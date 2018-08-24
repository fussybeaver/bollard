use std::env;
use std::path::{Path, PathBuf};
use std::str::from_utf8;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::result;
use futures::Stream;
use hyper::client::connect::Connect;
use hyper::client::HttpConnector;
use hyper::rt::Future;
use hyper::{self, header, Body, Client, Request, Response, StatusCode};
#[cfg(feature = "openssl")]
use hyper_openssl::HttpsConnector;
#[cfg(unix)]
use hyperlocal::{self, UnixConnector};
#[cfg(feature = "openssl")]
use openssl::pkcs12::Pkcs12;
#[cfg(feature = "openssl")]
use openssl::ssl::{SslConnector, SslConnectorBuilder};
#[cfg(feature = "openssl")]
use openssl::ssl::{SslContext, SslFiletype, SslMethod};
use tokio::timer::{Deadline, DeadlineError};
use tokio::util::FutureExt;

use container::{Container, ContainerInfo};
use errors::*;
use failure::Error;
use filesystem::FilesystemChange;
use image::{Image, ImageStatus};
use named_pipe::{self, NamedPipeConnector};
use options::*;
use process::{Process, Top};
use stats::Stats;
use system::SystemInfo;
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
pub fn default_cert_path() -> Result<PathBuf, Error> {
    let from_env = env::var("DOCKER_CERT_PATH").or_else(|_| env::var("DOCKER_CONFIG"));
    if let Ok(ref path) = from_env {
        Ok(Path::new(path).to_owned())
    } else {
        let home = env::home_dir().ok_or_else(|| NoCertPathError {})?;
        Ok(home.join(".docker"))
    }
}

enum ClientType {
    Unix,
    Tcp,
    NamedPipe,
}

fn arrayify(s: &str) -> String {
    let wrapped = format!("[{}]", s);
    wrapped.clone().replace("}\r\n{", "}{").replace("}{", "},{")
}

fn decode_response<T>(response: Response<Body>) -> impl Future<Item = T, Error = Error>
where
    T: DeserializeOwned,
{
    response
        .into_body()
        .concat2()
        .map_err(|e| e.into())
        .and_then(|body| from_utf8(&body).map(|x| x.to_owned()).map_err(|e| e.into()))
        .and_then(|contents| serde_json::from_str::<T>(&contents).map_err(|e| e.into()))
}

pub struct Docker<C> {
    client: Arc<Client<C>>,
    client_type: ClientType,
    client_addr: String,
    client_timeout: u64,
}

const DEFAULT_NUM_THREADS: usize = 4;
const DEFAULT_TIMEOUT: u64 = 4000;

#[cfg(feature = "openssl")]
impl Docker<HttpsConnector<HttpConnector>> {
    /// Connect to the Docker daemon using the standard Docker
    /// configuration options.  This includes `DOCKER_HOST`,
    /// `DOCKER_TLS_VERIFY`, `DOCKER_CERT_PATH` and `DOCKER_CONFIG`, and we
    /// try to interpret these as much like the standard `docker` client as
    /// possible.
    pub fn new() -> Result<Docker<HttpsConnector<HttpConnector>>, Error> {
        let host = env::var("DOCKER_HOST").unwrap_or(DEFAULT_DOCKER_HOST.to_string());
        let cert_path = default_cert_path()?;

        Docker::connect_with_ssl(
            &host,
            &cert_path.join("key.pem"),
            &cert_path.join("cert.pem"),
            &cert_path.join("ca.pem"),
            DEFAULT_NUM_THREADS,
            DEFAULT_TIMEOUT,
        )
    }

    pub fn connect_with_ssl(
        addr: &str,
        ssl_key: &Path,
        ssl_cert: &Path,
        ssl_ca: &Path,
        num_threads: usize,
        timeout: u64,
    ) -> Result<Docker<HttpsConnector<HttpConnector>>, Error> {
        // This ensures that using docker-machine-esque addresses work with Hyper.
        let client_addr = addr.clone().replace("tcp://", "https://");

        let mut ssl_connector_builder = SslConnector::builder(SslMethod::tls())?;

        ssl_connector_builder.set_ca_file(ssl_ca)?;
        ssl_connector_builder.set_certificate_file(ssl_cert, SslFiletype::PEM)?;
        ssl_connector_builder.set_private_key_file(ssl_key, SslFiletype::PEM)?;

        let mut http_connector = HttpConnector::new(num_threads);
        http_connector.enforce_http(false);

        let mut https_connector: HttpsConnector<HttpConnector> =
            HttpsConnector::with_connector(http_connector, ssl_connector_builder)?;

        let client_builder = Client::builder();
        let client = client_builder.build(https_connector);
        let docker = Docker {
            client: Arc::new(client),
            client_type: ClientType::Tcp,
            client_addr: client_addr.to_owned(),
            client_timeout: timeout,
        };

        Ok(docker)
    }
}

impl Docker<HttpConnector> {
    pub fn new() -> Result<Docker<HttpConnector>, Error> {
        // Read in our configuration from the Docker environment.
        let host = env::var("DOCKER_HOST").unwrap_or(DEFAULT_DOCKER_HOST.to_string());
        Docker::connect_with_http(&host, DEFAULT_NUM_THREADS, DEFAULT_TIMEOUT)
    }

    /// Connect using unsecured HTTP.  This is strongly discouraged
    /// everywhere but on Windows when npipe support is not available.
    pub fn connect_with_http(
        addr: &str,
        num_threads: usize,
        timeout: u64,
    ) -> Result<Docker<HttpConnector>, Error> {
        // This ensures that using docker-machine-esque addresses work with Hyper.
        let client_addr = addr.clone().replace("tcp://", "http://");

        let http_connector = HttpConnector::new(num_threads);

        let client_builder = Client::builder();
        let client = client_builder.build(http_connector);
        let docker = Docker {
            client: Arc::new(client),
            client_type: ClientType::Tcp,
            client_addr: client_addr.to_owned(),
            client_timeout: timeout,
        };

        Ok(docker)
    }
}

#[cfg(unix)]
impl Docker<UnixConnector> {
    pub fn new() -> Result<Docker<UnixConnector>, Error> {
        let host = env::var("DOCKER_HOST").unwrap_or(DEFAULT_DOCKER_HOST.to_string());
        Docker::connect_with_unix(&host, DEFAULT_TIMEOUT)
    }

    pub fn connect_with_unix(addr: &str, timeout: u64) -> Result<Docker<UnixConnector>, Error> {
        let client_addr = addr.clone().replace("unix://", "");

        let unix_connector = UnixConnector::new();

        let mut client_builder = Client::builder();
        client_builder.keep_alive(false);
        let client = client_builder.build(unix_connector);
        let docker = Docker {
            client: Arc::new(client),
            client_type: ClientType::Unix,
            client_addr: client_addr.to_owned(),
            client_timeout: timeout,
        };

        Ok(docker)
    }
}

#[cfg(windows)]
impl Docker<NamedPipeConnector> {
    pub fn new() -> Result<Docker<NamedPipeConnector>, Error> {
        // TODO: check if this environment is relevant here
        let addr = env::var("DOCKER_HOST").unwrap_or(DEFAULT_DOCKER_HOST.to_string());
        Docker::connect_with_named_pipe(&addr, DEFAULT_TIMEOUT)
    }

    pub fn connect_with_named_pipe(
        addr: &str,
        timeout: u64,
    ) -> Result<Docker<NamedPipeConnector>, Error> {
        let client_addr = addr.clone();

        let named_pipe_connector = NamedPipeConnector::new();

        let mut client_builder = Client::builder();
        client_builder.keep_alive(false);
        client_builder.http1_title_case_headers(true);
        let client = client_builder.build(named_pipe_connector);
        let docker = Docker {
            client: Arc::new(client),
            client_type: ClientType::NamedPipe,
            client_addr: client_addr.to_owned(),
            client_timeout: timeout,
        };

        Ok(docker)
    }
}

impl<'docker, C> Docker<C>
where
    C: Connect + Sync + 'static,
    C::Error: 'static,
    C::Transport: 'static,
{
    fn build_request(&self, path: &str) -> Result<Request<Body>, Error> {
        match self.client_type {
            ClientType::Tcp => {
                let mut request_url = self.client_addr.clone();
                request_url.push_str(path);
                Ok(Request::get(request_url).body(Body::empty())?)
            }
            ClientType::Unix => {
                #[cfg(unix)]
                {
                    let request_uri: hyper::Uri =
                        hyperlocal::Uri::new(self.client_addr.clone(), path).into();
                    Ok(Request::get(request_uri).body(Body::empty())?)
                }
                #[cfg(not(unix))]
                {
                    unreachable!();
                }
            }
            ClientType::NamedPipe => {
                let request_uri: hyper::Uri =
                    named_pipe::Uri::new(self.client_addr.clone(), path).into();
                Ok(Request::get(request_uri)
                    .header(header::CONTENT_TYPE, "text/plain")
                    .header(header::ACCEPT, "text/plain")
                    .header(header::USER_AGENT, "Docker.boondock")
                    .header(header::CONNECTION, "upgrade")
                    .header(header::UPGRADE, "tcp")
                    .body(Body::empty())?)
            }
        }
    }

    fn execute_request(
        client: Arc<Client<C>>,
        request: Request<Body>,
        timeout: Duration,
    ) -> impl Future<Item = Response<Body>, Error = Error> {
        let now = Instant::now();

        client
            .request(request)
            .deadline(now + timeout)
            .map_err(|e| e.into())
    }

    pub fn containers(
        &self,
        opts: ContainerListOptions,
    ) -> impl Future<Item = Vec<Container>, Error = Error> {
        let url = format!("/containers/json?{}", opts.to_url_params());

        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        result(self.build_request(&url))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                // Status code 200 - 299
                s if s.is_success() => Ok(response),

                // Status code 400: Bad request
                StatusCode::BAD_REQUEST => Err(BadParameterError {}.into()),

                // All other status codes
                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(move |response| decode_response(response))
    }

    pub fn processes(
        &self,
        container: &Container,
    ) -> impl Future<Item = Vec<Process>, Error = Error> {
        let url = format!("/containers/{}/top", container.Id);

        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        let container_id = container.Id.clone();

        result(self.build_request(&url))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                StatusCode::NOT_FOUND => Err(ContainerNotFoundError { id: container_id }.into()),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(move |response| decode_response(response))
            .and_then(|top: Top| {
                let mut processes: Vec<Process> = Vec::new();
                let mut process_iter = top.Processes.iter();
                loop {
                    let process = match process_iter.next() {
                        Some(process) => process,
                        None => {
                            break;
                        }
                    };

                    let mut p = Process {
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
                        command: String::new(),
                    };

                    let mut value_iter = process.iter();
                    let mut i: usize = 0;
                    loop {
                        let value = match value_iter.next() {
                            Some(value) => value,
                            None => {
                                break;
                            }
                        };
                        let key = &top.Titles[i];
                        match key.as_ref() {
                            "UID" => p.user = value.clone(),
                            "USER" => p.user = value.clone(),
                            "PID" => p.pid = value.clone(),
                            "%CPU" => p.cpu = Some(value.clone()),
                            "%MEM" => p.memory = Some(value.clone()),
                            "VSZ" => p.vsz = Some(value.clone()),
                            "RSS" => p.rss = Some(value.clone()),
                            "TTY" => p.tty = Some(value.clone()),
                            "STAT" => p.stat = Some(value.clone()),
                            "START" => p.start = Some(value.clone()),
                            "STIME" => p.start = Some(value.clone()),
                            "TIME" => p.time = Some(value.clone()),
                            "CMD" => p.command = value.clone(),
                            "COMMAND" => p.command = value.clone(),
                            _ => {}
                        }

                        i = i + 1;
                    }
                    processes.push(p);
                }

                Ok(processes)
            })
    }

    pub fn stats(&self, container: &Container) -> impl Future<Item = Stats, Error = Error> {
        let url = format!("/containers/{}/stats", container.Id);

        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        let container_id = container.Id.clone();

        result(self.build_request(&url))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                StatusCode::NOT_FOUND => Err(ContainerNotFoundError { id: container_id }.into()),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(|response| decode_response(response))
    }

    pub fn create_image(
        &'static self,
        image: String,
        tag: String,
    ) -> impl Future<Item = Vec<ImageStatus>, Error = Error> {
        let url = format!("/images/create?fromImage={}&tag={}", image, tag);

        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        result(self.build_request(&url))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                StatusCode::NOT_FOUND => Err(ReadError {}.into()),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(|response| decode_response(response))
    }

    pub fn images(&self, all: bool) -> impl Future<Item = Vec<Image>, Error = Error> {
        let a = match all {
            true => "1",
            false => "0",
        };
        let url = format!("/images/json?all={}", a);

        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        result(self.build_request(&url))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(|response| decode_response(response))
    }

    pub fn system_info(&self) -> impl Future<Item = SystemInfo, Error = Error> {
        let url = format!("/info");

        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        result(self.build_request(&url))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(|response| decode_response(response))
    }

    pub fn container_info(
        &self,
        container: &Container,
    ) -> impl Future<Item = ContainerInfo, Error = Error> {
        let url = format!("/containers/{}/json", container.Id);

        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        let container_id = container.Id.clone();

        result(self.build_request(&url))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                StatusCode::NOT_FOUND => Err(ContainerNotFoundError { id: container_id }.into()),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(|response| decode_response(response))
    }

    pub fn filesystem_changes(
        &self,
        container: &Container,
    ) -> impl Future<Item = Vec<FilesystemChange>, Error = Error> {
        let url = format!("/containers/{}/changes", container.Id);

        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        let container_id = container.Id.clone();

        result(self.build_request(&url))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                StatusCode::NOT_FOUND => Err(ContainerNotFoundError { id: container_id }.into()),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(|response| decode_response(response))
    }

    pub fn ping(&self) -> impl Future<Item = (), Error = Error> {
        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        result(self.build_request("/_ping"))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(|_| Ok(()))
    }

    pub fn version(&self) -> impl Future<Item = Version, Error = Error> {
        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        result(self.build_request("/version"))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(|response| decode_response(response))
    }

    pub fn set_read_timeout(&mut self, duration: u64) {
        self.client_timeout = duration;
    }
}
