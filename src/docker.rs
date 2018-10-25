use std::env;
#[cfg(feature = "openssl")]
use std::path::{Path, PathBuf};
use std::str::from_utf8;
use std::sync::Arc;
use std::time::{Duration, Instant};

use failure::Error;
use futures::future::{self, result};
use futures::Stream;
use hyper::client::connect::Connect;
use hyper::client::HttpConnector;
use hyper::rt::Future;
use hyper::Method;
use hyper::{self, Body, Client, Request, Response, StatusCode};
#[cfg(feature = "openssl")]
use hyper_openssl::HttpsConnector;
#[cfg(unix)]
use hyperlocal::UnixConnector;
#[cfg(feature = "openssl")]
use openssl::pkcs12::Pkcs12;
#[cfg(feature = "openssl")]
use openssl::ssl::SslConnector;
#[cfg(feature = "openssl")]
use openssl::ssl::{SslFiletype, SslMethod};
use tokio::util::FutureExt;
use tokio_codec::FramedRead;

use errors::{
    DockerResponseBadParameterError, DockerResponseNotFoundError, DockerResponseServerError,
    JsonDataError,
};
#[cfg(windows)]
use named_pipe::{self, NamedPipeConnector};
use options::EncodableQueryString;
use options::NoParams;
use read::{JsonLineDecoder, StreamReader};
use uri::Uri;

use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde_json;

/// The default `DOCKER_SOCKET` address that we will try to connect to.
#[cfg(unix)]
pub const DEFAULT_SOCKET: &'static str = "unix:///var/run/docker.sock";

/// The default `DOCKER_NAMED_PIPE` address that a windows client will try to connect to.
#[cfg(windows)]
pub const DEFAULT_NAMED_PIPE: &'static str = "npipe:////./pipe/docker_engine";

/// The default `DOCKER_HOST` address that we will try to connect to.
pub const DEFAULT_DOCKER_HOST: &'static str = "tcp://localhost:2375";

const DEFAULT_NUM_THREADS: usize = 1;

/// Default timeout for all requests is 2 minutes.
const DEFAULT_TIMEOUT: u64 = 120000;

/// The default directory in which to look for our Docker certificate
/// files.
#[cfg(openssl)]
pub fn default_cert_path() -> Result<PathBuf, Error> {
    use errors::NoCertPathError;

    let from_env = env::var("DOCKER_CERT_PATH").or_else(|_| env::var("DOCKER_CONFIG"));
    if let Ok(ref path) = from_env {
        Ok(Path::new(path).to_owned())
    } else {
        let home = env::home_dir().ok_or_else(|| NoCertPathError {})?;
        Ok(home.join(".docker"))
    }
}

#[derive(Debug)]
pub enum ClientType {
    Unix,
    Http,
    Https,
    NamedPipe,
}
pub struct Docker<C> {
    client: Arc<Client<C>>,
    client_type: ClientType,
    client_addr: String,
    client_timeout: u64,
}

#[cfg(feature = "openssl")]
impl Docker<HttpsConnector<HttpConnector>> {
    pub fn connect_with_ssl_defaults() -> Result<Docker<HttpsConnector<HttpConnector>>, Error> {
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
        let client_addr = addr.clone().replace("tcp://", "");

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
            client_type: ClientType::Https,
            client_addr: client_addr.to_owned(),
            client_timeout: timeout,
        };

        Ok(docker)
    }
}

impl Docker<HttpConnector> {
    /// Connect using unsecured HTTP.
    ///
    /// # Defaults
    ///
    ///  - The connection url is sourced from the `DOCKER_HOST` environment variable.
    ///  - The number of threads used for the HTTP connection pool defaults to 1.
    ///  - The request timeout defaults to 2 minutes.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate boondock;
    /// # extern crate futures;
    /// # fn main () {
    /// use boondock::Docker;
    ///
    /// use futures::future::Future;
    ///
    /// let connection = Docker::connect_with_http_defaults().unwrap();
    /// connection.ping()
    ///   .and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_http_defaults() -> Result<Docker<impl Connect>, Error> {
        let host = env::var("DOCKER_HOST").unwrap_or(DEFAULT_DOCKER_HOST.to_string());
        Docker::connect_with_http(&host, DEFAULT_NUM_THREADS, DEFAULT_TIMEOUT)
    }

    /// Connect using unsecured HTTP.  
    ///
    /// # Arguments
    ///
    ///  - `addr`: connection url including scheme and port.
    ///  - `num_threads`: the number of threads for the HTTP connection pool.
    ///  - `timeout`: the read/write timeout to use for every hyper connection
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate boondock;
    /// # extern crate futures;
    /// # fn main () {
    /// use boondock::Docker;
    ///
    /// use futures::future::Future;
    ///
    /// let connection = Docker::connect_with_http(
    ///                    "http://my-custom-docker-server:2735", 4, 20)
    ///                    .unwrap();
    /// connection.ping()
    ///   .and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_http(
        addr: &str,
        num_threads: usize,
        timeout: u64,
    ) -> Result<Docker<impl Connect>, Error> {
        // This ensures that using docker-machine-esque addresses work with Hyper.
        let client_addr = addr.clone().replace("tcp://", "");

        let http_connector = HttpConnector::new(num_threads);

        let mut client_builder = Client::builder();
        client_builder.keep_alive(true);
        let client = client_builder.build(http_connector);
        let docker = Docker {
            client: Arc::new(client),
            client_type: ClientType::Http,
            client_addr: client_addr.to_owned(),
            client_timeout: timeout,
        };

        Ok(docker)
    }
}

#[cfg(unix)]
/// A Docker implementation typed to connect to a Unix socket.
impl Docker<UnixConnector> {
    /// Connect using a Unix socket.
    ///
    /// # Defaults
    ///
    ///  - The socket location defaults to `/var/run/docker.sock`.
    ///  - The number of threads used for the tokio executor defaults to 1.
    ///  - The request timeout defaults to 2 minutes.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// //use boondock::Docker;
    ///
    /// //let connection = Docker::connect_with_http_defaults().unwrap();
    /// //connection.ping().and_then(|_| println!("Connected!"));
    /// ```
    pub fn connect_with_unix_defaults() -> Result<Docker<impl Connect>, Error> {
        let host = DEFAULT_SOCKET.to_string();
        Docker::connect_with_unix(&host, DEFAULT_TIMEOUT)
    }

    pub fn connect_with_unix(addr: &str, timeout: u64) -> Result<Docker<impl Connect>, Error> {
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
    pub fn connect_with_named_pipe_defaults() -> Result<Docker<NamedPipeConnector>, Error> {
        let addr = DEFAULT_NAMED_PIPE.to_string();
        Docker::connect_with_named_pipe(&addr, DEFAULT_TIMEOUT)
    }

    pub fn connect_with_named_pipe(
        addr: &str,
        timeout: u64,
    ) -> Result<Docker<NamedPipeConnector>, Error> {
        let client_addr = addr.clone().replace("npipe://", "");

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

impl<C> Docker<C>
where
    C: Connect + Sync + 'static,
{
    pub fn connect_with(connector: C, client_addr: String) -> Result<Docker<C>, Error> {
        let timeout = 5;

        let mut client_builder = Client::builder();
        client_builder.keep_alive(false);
        let client = client_builder.build(connector);

        let docker = Docker {
            client: Arc::new(client),
            client_type: ClientType::Http,
            client_addr,
            client_timeout: timeout,
        };

        Ok(docker)
    }
}

enum Either4<B, C, D> {
    A(future::FutureResult<Response<Body>, Error>),
    B(B),
    C(C),
    D(D),
}

impl<B, C, D> Future for Either4<B, C, D>
where
    B: Future<Item = Response<Body>, Error = Error>,
    C: Future<Item = Response<Body>, Error = Error>,
    D: Future<Item = Response<Body>, Error = Error>,
{
    type Item = Response<Body>;
    type Error = Error;

    fn poll(&mut self) -> ::futures::Poll<Response<Body>, Error> {
        match *self {
            Either4::A(ref mut a) => a.poll(),
            Either4::B(ref mut b) => b.poll(),
            Either4::C(ref mut c) => c.poll(),
            Either4::D(ref mut d) => d.poll(),
        }
    }
}

impl<C> Docker<C>
where
    C: Connect + Sync + 'static,
{
    /*
    pub(crate) fn process_into_value2<O, T, S, K, V>(
        &self,
        url: &str,
        method: Method,
        params: Option<O>,
        body: Option<S>,
    ) -> impl Future<Item = T, Error = Error>
    where
        O: IntoIterator,
        O::Item: ::std::borrow::Borrow<(K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
        T: DeserializeOwned,
        S: Serialize,
    {
        self.process_request2(url, method, params, body)
            .and_then(Docker::<C>::decode_response)
    }
    */

    pub(crate) fn process_into_value<O, T, S>(
        &self,
        url: &str,
        method: Method,
        params: Option<O>,
        body: Option<S>,
    ) -> impl Future<Item = T, Error = Error>
    where
        O: EncodableQueryString,
        T: DeserializeOwned,
        S: Serialize,
    {
        self.process_request(url, method, params, body)
            .and_then(Docker::<C>::decode_response)
    }

    pub(crate) fn process_into_stream3<T>(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Stream<Item = T, Error = Error>
    where
        T: DeserializeOwned,
    {
        self.process_request3(req)
            .into_stream()
            .map(Docker::<C>::decode_into_stream::<T>)
            .flatten()
    }

    pub(crate) fn process_into_stream2<O, T, S, K, V>(
        &self,
        url: &str,
        method: Method,
        params: Option<O>,
        body: Option<S>,
    ) -> impl Stream<Item = T, Error = Error>
    where
        O: IntoIterator,
        O::Item: ::std::borrow::Borrow<(K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
        T: DeserializeOwned,
        S: Serialize,
    {
        self.process_request2(url, method, params, body)
            .into_stream()
            .map(Docker::<C>::decode_into_stream::<T>)
            .flatten()
    }

    pub(crate) fn process_into_stream<O, T, S>(
        &self,
        url: &str,
        method: Method,
        params: Option<O>,
        body: Option<S>,
    ) -> impl Stream<Item = T, Error = Error>
    where
        O: EncodableQueryString,
        T: DeserializeOwned,
        S: Serialize,
    {
        self.process_request(url, method, params, body)
            .into_stream()
            .map(Docker::<C>::decode_into_stream::<T>)
            .flatten()
    }

    pub(crate) fn process_into_void<O, S>(
        &self,
        url: &str,
        method: Method,
        params: Option<O>,
        body: Option<S>,
    ) -> impl Future<Item = (), Error = Error>
    where
        O: EncodableQueryString,
        S: Serialize,
    {
        self.process_request(url, method, params, body)
            .and_then(|_| Ok(()))
    }

    fn process_request3(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Future<Item = Response<Body>, Error = Error> {
        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        result(req)
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| {
                let status = response.status();
                match status {
                    // Status code 200 - 299
                    s if s.is_success() => Either4::A(future::ok(response)),

                    // Status code 400: Bad request
                    StatusCode::BAD_REQUEST => Either4::D(
                        Docker::<C>::decode_into_string(response).and_then(|message| {
                            Err(DockerResponseBadParameterError { message }.into())
                        }),
                    ),

                    // Status code 404: Not Found
                    StatusCode::NOT_FOUND => Either4::C(
                        Docker::<C>::decode_into_string(response).and_then(|message| {
                            Err(DockerResponseNotFoundError { message }.into())
                        }),
                    ),

                    // All other status codes
                    _ => Either4::B(Docker::<C>::decode_into_string(response).and_then(
                        move |message| {
                            Err(DockerResponseServerError {
                                status_code: status.as_u16(),
                                message,
                            }.into())
                        },
                    )),
                }
            })
    }

    fn process_request2<O, S, K, V>(
        &self,
        url: &str,
        method: Method,
        params: Option<O>,
        body: Option<S>,
    ) -> impl Future<Item = Response<Body>, Error = Error>
    where
        O: IntoIterator,
        O::Item: ::std::borrow::Borrow<(K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
        S: Serialize,
    {
        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        let des_request_body = match body.map(|inst| serde_json::to_string(&inst)) {
            Some(Ok(res)) => Ok(Some(res)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }.map_err(|e| e.into());

        result(des_request_body.and_then(|payload| {
            println!("{}", payload.clone().unwrap_or_else(String::new));
            let body = payload
                .map(|content| content.into())
                .unwrap_or(Body::empty());
            self.build_request2(url, method, params, body)
        })).and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| {
                let status = response.status();
                match status {
                    // Status code 200 - 299
                    s if s.is_success() => Either4::A(future::ok(response)),

                    // Status code 400: Bad request
                    StatusCode::BAD_REQUEST => Either4::D(
                        Docker::<C>::decode_into_string(response).and_then(|message| {
                            Err(DockerResponseBadParameterError { message }.into())
                        }),
                    ),

                    // Status code 404: Not Found
                    StatusCode::NOT_FOUND => Either4::C(
                        Docker::<C>::decode_into_string(response).and_then(|message| {
                            Err(DockerResponseNotFoundError { message }.into())
                        }),
                    ),

                    // All other status codes
                    _ => Either4::B(Docker::<C>::decode_into_string(response).and_then(
                        move |message| {
                            Err(DockerResponseServerError {
                                status_code: status.as_u16(),
                                message,
                            }.into())
                        },
                    )),
                }
            })
    }

    pub(crate) fn transpose_option<T>(
        option: Option<Result<T, Error>>,
    ) -> Result<Option<T>, Error> {
        match option {
            Some(Ok(x)) => Ok(Some(x)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub(crate) fn serialize_payload<S>(body: Option<S>) -> Result<Body, Error>
    where
        S: Serialize,
    {
        match body.map(|inst| serde_json::to_string(&inst)) {
            Some(Ok(res)) => Ok(Some(res)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }.map_err(|e| e.into())
            .map(|payload| {
                println!("{}", payload.clone().unwrap_or_else(String::new));
                payload
                    .map(|content| content.into())
                    .unwrap_or(Body::empty())
            })
    }

    fn process_request<O, S>(
        &self,
        url: &str,
        method: Method,
        params: Option<O>,
        body: Option<S>,
    ) -> impl Future<Item = Response<Body>, Error = Error>
    where
        O: EncodableQueryString,
        S: Serialize,
    {
        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        let des_request_body = match body.map(|inst| serde_json::to_string(&inst)) {
            Some(Ok(res)) => Ok(Some(res)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }.map_err(|e| e.into());

        result(des_request_body.and_then(|payload| {
            println!("{}", payload.clone().unwrap_or_else(String::new));
            let body = payload
                .map(|content| content.into())
                .unwrap_or(Body::empty());
            self.build_request(url, method, params, body)
        })).and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| {
                let status = response.status();
                match status {
                    // Status code 200 - 299
                    s if s.is_success() => Either4::A(future::ok(response)),

                    // Status code 400: Bad request
                    StatusCode::BAD_REQUEST => Either4::D(
                        Docker::<C>::decode_into_string(response).and_then(|message| {
                            Err(DockerResponseBadParameterError { message }.into())
                        }),
                    ),

                    // Status code 404: Not Found
                    StatusCode::NOT_FOUND => Either4::C(
                        Docker::<C>::decode_into_string(response).and_then(|message| {
                            Err(DockerResponseNotFoundError { message }.into())
                        }),
                    ),

                    // All other status codes
                    _ => Either4::B(Docker::<C>::decode_into_string(response).and_then(
                        move |message| {
                            Err(DockerResponseServerError {
                                status_code: status.as_u16(),
                                message,
                            }.into())
                        },
                    )),
                }
            })
    }

    pub(crate) fn build_request3<O, K, V>(
        &self,
        path: &str,
        method: Method,
        query: Result<Option<O>, Error>,
        payload: Result<Body, Error>,
    ) -> Result<Request<Body>, Error>
    where
        O: IntoIterator,
        O::Item: ::std::borrow::Borrow<(K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        query
            .and_then(|q| payload.map(|body| (q, body)))
            .and_then(|(q, body)| {
                let uri = Uri::parse2(self.client_addr.clone(), &self.client_type, path, q)?;
                let request_uri: hyper::Uri = uri.into();
                match method {
                    Method::GET => Ok(Request::get(request_uri).body(body)?),
                    Method::POST => Ok(Request::post(request_uri)
                        .header("content-type", "application/json")
                        .body(body)?),
                    Method::DELETE => Ok(Request::delete(request_uri).body(body)?),
                    _ => unreachable!(),
                }
            })
    }

    pub(crate) fn build_request2<O, K, V>(
        &self,
        path: &str,
        method: Method,
        query: Option<O>,
        payload: Body,
    ) -> Result<Request<Body>, Error>
    where
        O: IntoIterator,
        O::Item: ::std::borrow::Borrow<(K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let uri = Uri::parse2(self.client_addr.clone(), &self.client_type, path, query)?;
        let request_uri: hyper::Uri = uri.into();
        match method {
            Method::GET => Ok(Request::get(request_uri).body(payload)?),
            Method::POST => Ok(Request::post(request_uri)
                .header("Content-Type", "application/json")
                .body(payload)?),
            Method::DELETE => Ok(Request::delete(request_uri).body(payload)?),
            _ => unreachable!(),
        }
    }

    fn build_request<O>(
        &self,
        path: &str,
        method: Method,
        query: Option<O>,
        payload: Body,
    ) -> Result<Request<Body>, Error>
    where
        O: EncodableQueryString,
    {
        let uri = Uri::parse(self.client_addr.clone(), &self.client_type, path, query)?;
        let request_uri: hyper::Uri = uri.into();
        match method {
            Method::GET => Ok(Request::get(request_uri).body(payload)?),
            Method::POST => Ok(Request::post(request_uri)
                .header("Content-Type", "application/json")
                .body(payload)?),
            Method::DELETE => Ok(Request::delete(request_uri).body(payload)?),
            _ => unreachable!(),
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

    fn decode_into_stream<T>(res: Response<Body>) -> impl Stream<Item = T, Error = Error>
    where
        T: DeserializeOwned,
    {
        FramedRead::new(
            StreamReader::new(res.into_body().from_err()),
            JsonLineDecoder::new(),
        )
    }

    fn decode_into_string(response: Response<Body>) -> impl Future<Item = String, Error = Error> {
        response
            .into_body()
            .concat2()
            .map_err(|e| e.into())
            .and_then(|body| from_utf8(&body).map(|x| x.to_owned()).map_err(|e| e.into()))
    }

    fn decode_response<T>(response: Response<Body>) -> impl Future<Item = T, Error = Error>
    where
        T: DeserializeOwned,
    {
        Docker::<C>::decode_into_string(response).and_then(|contents| {
            println!("{}", &contents);
            serde_json::from_str::<T>(&contents).map_err(|e| {
                if e.is_data() {
                    JsonDataError {
                        message: e.to_string(),
                        column: e.column(),
                        contents: contents.to_owned(),
                    }.into()
                } else {
                    e.into()
                }
            })
        })
    }

    /*
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

                StatusCode::NOT_FOUND => Err(NotFoundError { id: container_id }.into()),

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

                StatusCode::NOT_FOUND => Err(NotFoundError { id: container_id }.into()),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(|response| decode_response(response))
    }
    pub fn list_images(
        &self,
        options: ListImageOptions,
    ) -> impl Future<Item = Vec<APIImages>, Error = Error> {
        let url = "/images/json";

        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        result(
            options
                .into_array()
                .and_then(|opts| self.build_request(&url, Method::GET, opts.iter())),
        ).and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(|response| decode_response(response))
    }

    /// Create an image by either pulling it from a registry or importing it.
    pub fn create_image(
        &self,
        options: CreateImageOptions,
    ) -> impl Stream<Item = JsonResponse, Error = Error> {
        let url = "/images/create";

        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        result(self.build_request(&url, Method::POST, options.into_array().iter()))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                StatusCode::NOT_FOUND => Err(NotFoundError {}.into()),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .into_stream()
            .map(|response| decode_into_stream::<JsonResponse>(response))
            .flatten()
    }

    pub fn inspect_image(&self, image_name: &str) -> impl Future<Item = Image, Error = Error> {
        let url = format!("/images/{}/json", image_name);

        let client = self.client.clone();
        let timeout = Duration::from_millis(self.client_timeout);

        result(self.build_request(&url, Method::GET, no_query.iter()))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                StatusCode::NOT_FOUND => Err(NotFoundError {}.into()),

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

        result(self.build_request(&url, Method::GET, no_query.iter()))
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

        result(self.build_request(&url, Method::GET, no_query.iter()))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                StatusCode::NOT_FOUND => Err(NotFoundError {}.into()),

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

        result(self.build_request(&url, Method::GET, no_query.iter()))
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| match response.status() {
                s if s.is_success() => Ok(response),

                StatusCode::NOT_FOUND => Err(NotFoundError {}.into()),

                _ => Err(DockerServerError {
                    status_code: response.status().as_u16(),
                }.into()),
            })
            .and_then(|response| decode_response(response))
    }
    */

    pub fn ping(&self) -> impl Future<Item = String, Error = Error> {
        let url = "/_ping";

        self.process_into_value(url, Method::GET, None::<NoParams>, None::<NoParams>)
    }
}
