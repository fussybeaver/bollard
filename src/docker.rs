use std;
use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::io::{self, Read};

use hyper;
use hyper::Client;
use hyper::client::RequestBuilder;
use hyper::client::pool::{Config, Pool};
use hyper::client::response::Response;
use hyper::net::HttpConnector;
#[cfg(feature="openssl")]
use hyper::net::{HttpsConnector, Openssl};

#[cfg(unix)]
use unix::HttpUnixConnector;

#[cfg(feature="openssl")]
use openssl::ssl::{SslContext, SslMethod};
#[cfg(feature="openssl")]
use openssl::ssl::error::SslError;
#[cfg(feature="openssl")]
use openssl::x509::X509FileType;

use errors::{self, ChainErr, ErrorKind};
use container::{Container, ContainerInfo};
use process::{Process, Top};
use stats::StatsReader;
use system::SystemInfo;
use image::{Image, ImageStatus};
use filesystem::FilesystemChange;
use version::Version;

use rustc_serialize::json;

/// The default `DOCKER_HOST` address that we will try to connect to.
#[cfg(unix)]
pub const DEFAULT_DOCKER_HOST: &'static str = "unix:///var/run/docker.sock";

/// The default `DOCKER_HOST` address that we will try to connect to.
///
/// This should technically be `"npipe:////./pipe/docker_engine"` on
/// Windows, but we don't support Windows pipes yet.  However, the TCP port
/// is still available.
#[cfg(windows)]
pub const DEFAULT_DOCKER_HOST: &'static str = "tcp://localhost:2375";

/// The default directory in which to look for our Docker certificate
/// files.
pub fn default_cert_path() -> errors::Result<PathBuf> {
    let from_env = env::var("DOCKER_CERT_PATH")
        .or_else(|_| env::var("DOCKER_CONFIG"));
    if let Ok(ref path) = from_env {
        Ok(Path::new(path).to_owned())
    } else {
        let home = try!(env::home_dir()
            .ok_or_else(|| ErrorKind::NoCertPath));
        Ok(home.join(".docker"))
    }
}

enum ClientType {
    Unix,
    Tcp,
}

pub struct Docker {
    client: Client,
    client_type: ClientType,
    client_addr: String,
}

impl Docker {
    /// Connect to the Docker daemon using the standard Docker
    /// configuration options.  This includes `DOCKER_HOST`,
    /// `DOCKER_TLS_VERIFY`, `DOCKER_CERT_PATH` and `DOCKER_CONFIG`, and we
    /// try to interpret these as much like the standard `docker` client as
    /// possible.
    pub fn connect_with_defaults() -> errors::Result<Docker> {
        // Read in our configuration from the Docker environment.
        let host = env::var("DOCKER_HOST")
            .unwrap_or(DEFAULT_DOCKER_HOST.to_string());
        let tls_verify = env::var("DOCKER_TLS_VERIFY").is_ok();
        let cert_path = try!(default_cert_path());

        // Dispatch to the correct connection function.
        let mkerr = || ErrorKind::CouldNotConnect(host.clone());
        if host.starts_with("unix://") {
            Docker::connect_with_unix(&host).chain_err(&mkerr)
        } else if host.starts_with("tcp://") {
            if tls_verify {
                Docker::connect_with_ssl(&host,
                                         &cert_path.join("key.pem"),
                                         &cert_path.join("cert.pem"),
                                         &cert_path.join("ca.pem"))
                    .chain_err(&mkerr)
            } else {
                Docker::connect_with_http(&host).chain_err(&mkerr)
            }
        } else {
            Err(ErrorKind::UnsupportedScheme(host.clone()).into())
        }
    }

    #[cfg(unix)]
    pub fn connect_with_unix(addr: &str) -> errors::Result<Docker> {
        // This ensures that using a fully-qualified path --
        // e.g. unix://.... -- works.  The unix socket provider expects a
        // Path, so we don't need scheme.
        //
        // TODO: Fix `replace` here and in other connect_* functions to only
        // replace at the beginning of the string.
        let client_addr = addr.clone().replace("unix://", "");

        let http_unix_connector = HttpUnixConnector::new(&client_addr);
        let connection_pool_config = Config { max_idle: 8 };
        let connection_pool = Pool::with_connector(connection_pool_config, http_unix_connector);

        let client = Client::with_connector(connection_pool);
        let docker = Docker { client: client, client_type: ClientType::Unix, client_addr: client_addr };

        return Ok(docker);
    }

    #[cfg(not(unix))]
    pub fn connect_with_unix(addr: &str) -> errors::Result<Docker> {
        Err(errors::ErrorKind::UnsupportedScheme(addr.to_owned()).into())
    }

    #[cfg(feature="openssl")]
    pub fn connect_with_ssl(addr: &str, ssl_key: &Path, ssl_cert: &Path, ssl_ca: &Path) -> errors::Result<Docker> {
        // This ensures that using docker-machine-esque addresses work with Hyper.
        let client_addr = addr.clone().replace("tcp://", "https://");

        let mkerr = || ErrorKind::SslError(addr.to_owned());
        let mut ssl_context = try!(SslContext::new(SslMethod::Sslv23)
            .chain_err(&mkerr));
        try!(ssl_context.set_CA_file(ssl_ca).chain_err(&mkerr));
        try!(ssl_context.set_certificate_file(ssl_cert, X509FileType::PEM)
            .chain_err(&mkerr));
        try!(ssl_context.set_private_key_file(ssl_key, X509FileType::PEM)
            .chain_err(&mkerr));

        let hyper_ssl_context = Openssl { context: Arc::new(ssl_context) };
        let https_connector = HttpsConnector::new(hyper_ssl_context);
        let connection_pool_config = Config { max_idle: 8 };
        let connection_pool = Pool::with_connector(connection_pool_config, https_connector);

        let client = Client::with_connector(connection_pool);
        let docker = Docker {
            client: client,
            client_type:
            ClientType::Tcp,
            client_addr: client_addr.to_owned(),
        };

        return Ok(docker);
    }

    #[cfg(not(feature="openssl"))]
    pub fn connect_with_ssl(addr: &str, ssl_key: &Path, ssl_cert: &Path, ssl_ca: &Path) -> errors::Result<Docker> {
        Err(errors::ErrorKind::SslDisabled.into())
    }

    /// Connect using unsecured HTTP.  This is strongly discouraged
    /// everywhere but on Windows when npipe support is not available.
    pub fn connect_with_http(addr: &str) -> Result<Docker, std::io::Error> {
        // This ensures that using docker-machine-esque addresses work with Hyper.
        let client_addr = addr.clone().replace("tcp://", "http://");

        let http_connector = HttpConnector::default();
        let connection_pool_config = Config { max_idle: 8 };
        let connection_pool = Pool::with_connector(connection_pool_config, http_connector);

        let client = Client::with_connector(connection_pool);
        let docker = Docker { client: client, client_type: ClientType::Tcp, client_addr: client_addr };

        return Ok(docker);

    }

    fn get_url(&self, path: String) -> String {
        let mut base = match self.client_type {
            ClientType::Tcp => self.client_addr.clone(),
            ClientType::Unix => {
                // We need a host so the HTTP headers can be generated, so we just spoof it and say
                // that we're talking to localhost.  The hostname doesn't matter one bit.
                "http://localhost/".to_string()
            }
        };
        let new_path = path.clone();
        base.push_str(&*new_path);

        base
    }

    fn build_get_request(&self, request_url: String) -> RequestBuilder {
        self.client.get(&*request_url)
    }

    fn build_post_request(&self, request_url: String) -> RequestBuilder {
        self.client.post(&*request_url)
    }

    fn execute_request(&self, request: RequestBuilder) -> Result<String, hyper::error::Error> {
        match request.send() {
            Ok(mut response) => {
                assert!(response.status.is_success());

                let mut body = String::new();
                match response.read_to_string(&mut body) {
                    Ok(_) => Ok(body),
                    Err(e) => Err(hyper::error::Error::Io(e))
                }
            },
            Err(e) => Err(e)
        }
    }

    fn start_request(&self, request: RequestBuilder) -> Result<Response, hyper::error::Error> {
        match request.send() {
            Ok(response) => {
                assert!(response.status.is_success());
                Ok(response)
            },
            Err(e) => Err(e)
        }
    }

    fn arrayify(&self, s: String) -> String {
        let wrapped = format!("[{}]", s);
        wrapped.clone().replace("}\r\n{", "}{").replace("}{", "},{")
    }

    pub fn get_containers(&self, all: bool) -> std::io::Result<Vec<Container>> {
        let a = match all {
            true => "1",
            false => "0"
        };

        let request_url = self.get_url(format!("/containers/json?a={}&size=1", a));
        let request = self.build_get_request(request_url);
        match self.execute_request(request) {
            Ok(body) => {
                match json::decode::<Vec<Container>>(&body) {
                    Ok(containers) => Ok(containers),
                    Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.description()))
                }
            },
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
    }

    pub fn get_processes(&self, container: &Container) -> std::io::Result<Vec<Process>> {
        let request_url = self.get_url(format!("/containers/{}/top", container.Id));
        let request = self.build_get_request(request_url);
        match self.execute_request(request) {
            Ok(body) => {
                match json::decode::<Top>(&body) {
                    Ok(top) => {
                        let mut processes: Vec<Process> = Vec::new();
                        let mut process_iter = top.Processes.iter();
                        loop {
                            let process = match process_iter.next() {
                                Some(process) => process,
                                None => { break; }
                            };

                            let mut p = Process{
                                user: String::new(),
                                pid: String::new(),
                                cpu: None,
                                memory: None,
                                vsz: None,
                                rss: None,
                                tty: None,
                                stat: None,
                                start: None,
                                time: None,
                                command: String::new()
                            };

                            let mut value_iter = process.iter();
                            let mut i: usize = 0;
                            loop {
                                let value = match value_iter.next() {
                                    Some(value) => value,
                                    None => { break; }
                                };
                                let key = &top.Titles[i];
                                match key.as_ref() {
                                "UID" => { p.user = value.clone() },
                                    "USER" => {p.user = value.clone() },
                                    "PID" => { p.pid = value.clone() },
                                    "%CPU" => { p.cpu = Some(value.clone()) },
                                    "%MEM" => { p.memory = Some(value.clone()) },
                                    "VSZ" => { p.vsz = Some(value.clone()) },
                                    "RSS" => { p.rss = Some(value.clone()) },
                                    "TTY" => { p.tty = Some(value.clone()) },
                                    "STAT" => { p.stat = Some(value.clone()) },
                                    "START" => { p.start = Some(value.clone()) },
                                    "STIME" => { p.start = Some(value.clone()) },
                                    "TIME" => { p.time = Some(value.clone()) },
                                    "CMD" => { p.command = value.clone() },
                                    "COMMAND" => { p.command = value.clone() },
                                    _ => {}
                                }

                                i = i + 1;
                            };

                            processes.push(p);
                        };

                        Ok(processes)
                    },
                    Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.description()))
                }
            },
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
    }

    pub fn get_stats(&self, container: &Container) -> std::io::Result<StatsReader> {
        if container.Status.contains("Up") == false {
            let err = std::io::Error::new(std::io::ErrorKind::InvalidInput, "The container is already stopped.");
            return Err(err);
        }

        let request_url = self.get_url(format!("/containers/{}/stats", container.Id));
        let request = self.build_get_request(request_url);
        match self.start_request(request) {
            Ok(response) => Ok(StatsReader::new(response)),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
    }

    pub fn create_image(&self, image: String, tag: String) -> std::io::Result<Vec<ImageStatus>> {
        let request_url = self.get_url(format!("/images/create?fromImage={}&tag={}", image, tag));
        let request = self.build_post_request(request_url);
        match self.execute_request(request) {
            Ok(body) => {
                let fixed = self.arrayify(body);
                match json::decode::<Vec<ImageStatus>>(&fixed) {
                    Ok(statuses) => Ok(statuses),
                    Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.description()))
                }
            },
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
    }

    pub fn get_images(&self, all: bool) -> std::io::Result<Vec<Image>> {
        let a = match all {
            true => "1",
            false => "0"
        };

        let request_url = self.get_url(format!("/images/json?a={}", a));
        let request = self.build_get_request(request_url);
        match self.execute_request(request) {
            Ok(body) => {
                match json::decode::<Vec<Image>>(&body) {
                    Ok(images) => Ok(images),
                    Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.description()))
                }
            },
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
    }

    pub fn get_system_info(&self) -> std::io::Result<SystemInfo> {
        let request_url = self.get_url(format!("/info"));
        let request = self.build_get_request(request_url);
        match self.execute_request(request) {
            Ok(body) => {
                match json::decode::<SystemInfo>(&body) {
                    Ok(info) => Ok(info),
                    Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.description()))
                }
            },
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
    }

    pub fn get_container_info(&self, container: &Container) -> std::io::Result<ContainerInfo> {
        let request_url = self.get_url(format!("/containers/{}/json", container.Id));
        let request = self.build_get_request(request_url);
        match self.execute_request(request) {
            Ok(body) => {
                match json::decode::<ContainerInfo>(&body) {
                    Ok(info) => Ok(info),
                    Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.description()))
                }
            },
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
    }

    pub fn get_filesystem_changes(&self, container: &Container) -> std::io::Result<Vec<FilesystemChange>> {
        let request_url = self.get_url(format!("/containers/{}/changes", container.Id));
        let request = self.build_get_request(request_url);
        match self.execute_request(request) {
            Ok(body) => {
                match json::decode::<Vec<FilesystemChange>>(&body) {
                    Ok(changes) => Ok(changes),
                    Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.description()))
                }
            },
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
    }

    pub fn export_container(&self, container: &Container) -> std::io::Result<Response> {
        let request_url = self.get_url(format!("/containers/{}/export", container.Id));
        let request = self.build_get_request(request_url);
        match self.start_request(request) {
            Ok(response) => Ok(response),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
    }

     pub fn ping(&self) -> std::io::Result<String> {
        let request_url = self.get_url(format!("/_ping"));
        let request = self.build_get_request(request_url);
        match self.execute_request(request) {
            Ok(body) => Ok(body),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
     }

    pub fn get_version(&self) -> std::io::Result<Version> {
        let request_url = self.get_url(format!("/version"));
        let request = self.build_get_request(request_url);
        match self.execute_request(request) {
            Ok(body) => {
                match json::decode::<Version>(&body) {
                    Ok(version) => Ok(version),
                    Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.description()))
                }
            },
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
    }
}
