use std;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::io::Read;

use hyper;
use hyper::Client;
use hyper::client::RequestBuilder;
use hyper::client::pool::{Config, Pool};
use hyper::client::response::Response;
#[cfg(feature="openssl")]
use hyper::net::HttpsConnector;
#[cfg(feature="openssl")]
use hyper::net::Openssl;

#[cfg(unix)]
use unix::HttpUnixConnector;

#[cfg(feature="openssl")]
use openssl::ssl::{SslContext, SslMethod};
#[cfg(feature="openssl")]
use openssl::ssl::error::SslError;
#[cfg(feature="openssl")]
use openssl::x509::X509FileType;

use container::{Container, ContainerInfo};
use process::{Process, Top};
use stats::StatsReader;
use system::SystemInfo;
use image::{Image, ImageStatus};
use filesystem::FilesystemChange;
use version::Version;

use rustc_serialize::json;

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
    #[cfg(unix)]
    pub fn connect_with_unix(addr: String) -> Result<Docker, std::io::Error> {
        // This ensures that using a fully-qualified path -- e.g. unix://.... -- works.  The unix
        // socket provider expects a Path, so we don't need scheme.
        let client_addr = addr.clone().replace("unix://", "");

        let http_unix_connector = HttpUnixConnector::new(&client_addr);
        let connection_pool_config = Config { max_idle: 8 };
        let connection_pool = Pool::with_connector(connection_pool_config, http_unix_connector);

        let client = Client::with_connector(connection_pool);
        let docker = Docker { client: client, client_type: ClientType::Unix, client_addr: client_addr };

        return Ok(docker);
    }

    #[cfg(feature="openssl")]
    pub fn connect_with_ssl(addr: String, ssl_key: &Path, ssl_cert: &Path, ssl_ca: &Path) -> Result<Docker, SslError> {
        // This ensures that using docker-machine-esque addresses work with Hyper.
        let client_addr = addr.clone().replace("tcp://", "https://");

        let mut ssl_context = try!(SslContext::new(SslMethod::Sslv23));
        try!(ssl_context.set_CA_file(ssl_ca));
        try!(ssl_context.set_certificate_file(ssl_cert, X509FileType::PEM));
        try!(ssl_context.set_private_key_file(ssl_key, X509FileType::PEM));

        let hyper_ssl_context = Openssl { context: Arc::new(ssl_context) };
        let https_connector = HttpsConnector::new(hyper_ssl_context);
        let connection_pool_config = Config { max_idle: 8 };
        let connection_pool = Pool::with_connector(connection_pool_config, https_connector);

        let client = Client::with_connector(connection_pool);
        let docker = Docker { client: client, client_type: ClientType::Tcp, client_addr: client_addr };

        return Ok(docker);
    }

    fn get_url(&mut self, path: String) -> String {
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

    pub fn get_containers(&mut self, all: bool) -> std::io::Result<Vec<Container>> {
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

    pub fn get_processes(&mut self, container: &Container) -> std::io::Result<Vec<Process>> {
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

    pub fn get_stats(&mut self, container: &Container) -> std::io::Result<StatsReader> {
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

    pub fn create_image(&mut self, image: String, tag: String) -> std::io::Result<Vec<ImageStatus>> {
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

    pub fn get_images(&mut self, all: bool) -> std::io::Result<Vec<Image>> {
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

    pub fn get_system_info(&mut self) -> std::io::Result<SystemInfo> {
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

    pub fn get_container_info(&mut self, container: &Container) -> std::io::Result<ContainerInfo> {
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

    pub fn get_filesystem_changes(&mut self, container: &Container) -> std::io::Result<Vec<FilesystemChange>> {
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

    pub fn export_container(&mut self, container: &Container) -> std::io::Result<Response> {
        let request_url = self.get_url(format!("/containers/{}/export", container.Id));
        let request = self.build_get_request(request_url);
        match self.start_request(request) {
            Ok(response) => Ok(response),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
    }

     pub fn ping(&mut self) -> std::io::Result<String> {
        let request_url = self.get_url(format!("/_ping"));
        let request = self.build_get_request(request_url);
        match self.execute_request(request) {
            Ok(body) => Ok(body),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description()))
        }
     }

    pub fn get_version(&mut self) -> std::io::Result<Version> {
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
