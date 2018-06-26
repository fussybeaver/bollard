use std;
use std::collections::BTreeMap;
use std::{env, time};
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
#[cfg(feature="openssl")]
use openssl::ssl::{SslContext, SslMethod};
#[cfg(feature="openssl")]
use openssl::ssl::error::SslError;
#[cfg(feature="openssl")]
use openssl::x509::X509FileType;
#[cfg(unix)]
use unix::HttpUnixConnector;

use errors::*;
use container::{Container, ContainerInfo};
use options::*;
use process::{Process, Top};
use stats::StatsReader;
use system::SystemInfo;
use image::{Image, ImageStatus};
use filesystem::FilesystemChange;
use version::Version;

use serde::de::DeserializeOwned;
use serde_json;

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
pub fn default_cert_path() -> Result<PathBuf> {
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
    pub fn connect_with_defaults() -> Result<Docker> {
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
    pub fn connect_with_unix(addr: &str) -> Result<Docker> {
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
    pub fn connect_with_unix(addr: &str) -> Result<Docker> {
        Err(ErrorKind::UnsupportedScheme(addr.to_owned()).into())
    }

    #[cfg(feature="openssl")]
    pub fn connect_with_ssl(addr: &str, ssl_key: &Path, ssl_cert: &Path, ssl_ca: &Path) -> Result<Docker> {
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
    pub fn connect_with_ssl(addr: &str, ssl_key: &Path, ssl_cert: &Path, ssl_ca: &Path) -> Result<Docker> {
        Err(ErrorKind::SslDisabled.into())
    }

    /// Connect using unsecured HTTP.  This is strongly discouraged
    /// everywhere but on Windows when npipe support is not available.
    pub fn connect_with_http(addr: &str) -> Result<Docker> {
        // This ensures that using docker-machine-esque addresses work with Hyper.
        let client_addr = addr.clone().replace("tcp://", "http://");

        let http_connector = HttpConnector::default();
        let connection_pool_config = Config { max_idle: 8 };
        let connection_pool = Pool::with_connector(connection_pool_config, http_connector);

        let client = Client::with_connector(connection_pool);
        let docker = Docker { client: client, client_type: ClientType::Tcp, client_addr: client_addr };

        return Ok(docker);

    }

    fn get_url(&self, path: &str) -> String {
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

    fn build_get_request(&self, request_url: &str) -> RequestBuilder {
        self.client.get(request_url)
    }

    fn build_post_request(&self, request_url: &str) -> RequestBuilder {
        self.client.post(request_url)
    }

    fn execute_request(&self, request: RequestBuilder) -> Result<String> {
        let mut response = try!(request.send());
        assert!(response.status.is_success());

        let mut body = String::new();
        try!(response.read_to_string(&mut body));
        Ok(body)
    }

    fn start_request(&self, request: RequestBuilder) -> Result<Response> {
        let response = try!(request.send());
        assert!(response.status.is_success());
        Ok(response)
    }

    fn arrayify(&self, s: &str) -> String {
        let wrapped = format!("[{}]", s);
        wrapped.clone().replace("}\r\n{", "}{").replace("}{", "},{")
    }

    /// `GET` a URL and decode it.
    fn decode_url<T>(&self, type_name: &'static str, url: &str) -> Result<T>
        where T: DeserializeOwned<>
    {
        let request_url = self.get_url(url);
        let request = self.build_get_request(&request_url);
        let body = try!(self.execute_request(request));
        let info = try!(serde_json::from_str::<T>(&body)
            .chain_err(|| ErrorKind::ParseError(type_name, body)));
        Ok(info)
    }

    pub fn containers(&self, opts: ContainerListOptions)
                      -> Result<Vec<Container>> {
        let url = format!("/containers/json?{}", opts.to_url_params());
        self.decode_url("Container", &url)
    }

    pub fn processes(&self, container: &Container) -> Result<Vec<Process>> {
        let url = format!("/containers/{}/top", container.Id);
        let top: Top = try!(self.decode_url("Top", &url));

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
            }
            processes.push(p);
        }

        Ok(processes)
    }

    pub fn stats(&self, container: &Container) -> Result<StatsReader> {
        if container.Status.contains("Up") == false {
            return Err("The container is already stopped.".into());
        }

        let request_url = self.get_url(&format!("/containers/{}/stats", container.Id));
        let request = self.build_get_request(&request_url);
        let response = try!(self.start_request(request));
        Ok(StatsReader::new(response))
    }

    pub fn create_image(&self, image: String, tag: String) -> Result<Vec<ImageStatus>> {
        let request_url = self.get_url(&format!("/images/create?fromImage={}&tag={}", image, tag));
        let request = self.build_post_request(&request_url);
        let body = try!(self.execute_request(request));
        let fixed = self.arrayify(&body);
        let statuses = try!(serde_json::from_str::<Vec<ImageStatus>>(&fixed)
            .chain_err(|| ErrorKind::ParseError("ImageStatus", fixed)));
        Ok(statuses)
    }

    pub fn images(&self, all: bool) -> Result<Vec<Image>> {
        let a = match all {
            true => "1",
            false => "0"
        };
        let url = format!("/images/json?a={}", a);
        self.decode_url("Image", &url)
    }

    pub fn system_info(&self) -> Result<SystemInfo> {
        self.decode_url("SystemInfo", &format!("/info"))
    }

    pub fn container_info(&self, container: &Container) -> Result<ContainerInfo> {
        let url = format!("/containers/{}/json", container.Id);
        self.decode_url("ContainerInfo", &url)
            .chain_err(|| ErrorKind::ContainerInfo(container.Id.clone()))
    }

    pub fn filesystem_changes(&self, container: &Container) -> Result<Vec<FilesystemChange>> {
        let url = format!("/containers/{}/changes", container.Id);
        self.decode_url("FilesystemChange", &url)
    }

    pub fn export_container(&self, container: &Container) -> Result<Response> {
        let url = format!("/containers/{}/export", container.Id);
        let request_url = self.get_url(&url);
        let request = self.build_get_request(&request_url);
        let response = try!(self.start_request(request));
        Ok(response)
    }

    pub fn ping(&self) -> Result<String> {
        let request_url = self.get_url(&format!("/_ping"));
        let request = self.build_get_request(&request_url);
        let body = try!(self.execute_request(request));
        Ok(body)
    }

    pub fn version(&self) -> Result<Version> {
        self.decode_url("Version", "/version")
    }

    pub fn set_read_timeout(&mut self, dur: Option<time::Duration>) {
        self.client.set_read_timeout(dur)
    }
}
