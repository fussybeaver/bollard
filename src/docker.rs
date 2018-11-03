use arrayvec::ArrayVec;
use std::env;
#[cfg(feature = "openssl")]
use std::path::{Path, PathBuf};
use std::str::from_utf8;
use std::sync::Arc;
use std::time::{Duration, Instant};

use failure::Error;
use futures::future::{self, result};
use futures::Stream;
use http::header::CONTENT_TYPE;
use http::request::Builder;
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

use container::LogOutput;
use either::EitherResponse;
use errors::{
    DockerResponseBadParameterError, DockerResponseNotFoundError, DockerResponseServerError,
    JsonDataError,
};
#[cfg(windows)]
use named_pipe::NamedPipeConnector;
use read::{JsonLineDecoder, LineDecoder, StreamReader};
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

pub(crate) const TRUE_STR: &'static str = "true";
pub(crate) const FALSE_STR: &'static str = "false";

/// The default directory in which to look for our Docker certificate
/// files.
#[cfg(feature = "openssl")]
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

#[derive(Debug, Clone)]
pub enum ClientType {
    Unix,
    Http,
    SSL,
    NamedPipe,
}

/// ---
/// # Docker
///
/// The main interface for calling the Docker API. Construct a new Docker instance using one of the
/// connect methods:
///  - [`Docker::connect_with_http_defaults`](struct.Docker.html#method.connect_with_http_defaults)
///  - [`Docker::connect_with_named_pipe_defaults`](struct.Docker.html#method.connect_with_pipe_defaults)
///  - [`Docker::connect_with_ssl_defaults`](struct.Docker.html#method.connect_with_ssl_defaults)
///  - [`Docker::connect_with_unix_defaults`](struct.Docker.html#method.connect_with_unix_defaults)
///
pub struct Docker<C> {
    pub(crate) client: Arc<Client<C>>,
    pub(crate) client_type: ClientType,
    pub(crate) client_addr: String,
    pub(crate) client_timeout: Duration,
}

impl<C> Clone for Docker<C> {
    fn clone(&self) -> Docker<C> {
        Docker {
            client: self.client.clone(),
            client_type: self.client_type.clone(),
            client_addr: self.client_addr.clone(),
            client_timeout: self.client_timeout.clone(),
        }
    }
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
            Duration::from_millis(DEFAULT_TIMEOUT),
        )
    }

    pub fn connect_with_ssl(
        addr: &str,
        ssl_key: &Path,
        ssl_cert: &Path,
        ssl_ca: &Path,
        num_threads: usize,
        timeout: Duration,
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
            client_type: ClientType::SSL,
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
        Docker::connect_with_http(
            &host,
            DEFAULT_NUM_THREADS,
            Duration::from_millis(DEFAULT_TIMEOUT),
        )
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
    /// use std::time::Duration;
    ///
    /// let connection = Docker::connect_with_http(
    ///                    "http://my-custom-docker-server:2735", 4, Duration::from_secs(20))
    ///                    .unwrap();
    /// connection.ping()
    ///   .and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_http(
        addr: &str,
        num_threads: usize,
        timeout: Duration,
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
        Docker::connect_with_unix(&host, Duration::from_millis(DEFAULT_TIMEOUT))
    }

    pub fn connect_with_unix(addr: &str, timeout: Duration) -> Result<Docker<impl Connect>, Error> {
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
        Docker::connect_with_named_pipe(&addr, Duration::from_millis(DEFAULT_TIMEOUT))
    }

    pub fn connect_with_named_pipe(
        addr: &str,
        timeout: Duration,
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
        let timeout = Duration::from_secs(5);

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

/// ---
/// # DockerChain
///
/// Retains the same API as the [Docker
/// Client](struct.Docker.html), but consumes the instance and returns the
/// instance as part of the response.
///
/// # Examples
///
/// ```rust,norun
/// use boondock::Docker;
/// let docker = Docker::connect_with_http_defaults().unwrap();
/// docker.chain();
/// ```
pub struct DockerChain<C> {
    pub(crate) inner: Docker<C>,
}

impl<C> Docker<C>
where
    C: Connect + Sync + 'static,
{
    pub fn chain(self) -> DockerChain<C> {
        DockerChain { inner: self }
    }

    pub(crate) fn process_into_value2<T>(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Future<Item = T, Error = Error>
    where
        T: DeserializeOwned,
    {
        self.process_request2(req)
            .and_then(Docker::<C>::decode_response)
    }

    pub(crate) fn process_into_stream2<T>(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Stream<Item = T, Error = Error>
    where
        T: DeserializeOwned,
    {
        self.process_request2(req)
            .into_stream()
            .map(Docker::<C>::decode_into_stream::<T>)
            .flatten()
    }

    pub(crate) fn process_into_stream_string2(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Stream<Item = LogOutput, Error = Error> {
        self.process_request2(req)
            .into_stream()
            .map(Docker::<C>::decode_into_stream_string)
            .flatten()
    }

    pub(crate) fn process_into_unit(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Future<Item = (), Error = Error> {
        self.process_request2(req).and_then(|_| Ok(()))
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

    fn process_request2(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Future<Item = Response<Body>, Error = Error> {
        let client = self.client.clone();
        let timeout = self.client_timeout.clone();

        result(req)
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| {
                let status = response.status();
                match status {
                    // Status code 200 - 299
                    s if s.is_success() => EitherResponse::A(future::ok(response)),

                    // Status code 400: Bad request
                    StatusCode::BAD_REQUEST => {
                        EitherResponse::D(Docker::<C>::decode_into_string(response).and_then(
                            |message| Err(DockerResponseBadParameterError { message }.into()),
                        ))
                    }

                    // Status code 404: Not Found
                    StatusCode::NOT_FOUND => {
                        EitherResponse::C(Docker::<C>::decode_into_string(response).and_then(
                            |message| Err(DockerResponseNotFoundError { message }.into()),
                        ))
                    }

                    // All other status codes
                    _ => EitherResponse::B(Docker::<C>::decode_into_string(response).and_then(
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

    pub(crate) fn build_request2<O, K, V>(
        &self,
        path: &str,
        builder: &mut Builder,
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
                Ok(builder
                    .uri(request_uri)
                    .header(CONTENT_TYPE, "application/json")
                    .body(body)?)
            })
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

    fn decode_into_stream_string(
        res: Response<Body>,
    ) -> impl Stream<Item = LogOutput, Error = Error> {
        FramedRead::new(
            StreamReader::new(res.into_body().from_err()),
            LineDecoder::new(),
        ).map_err(|e| {
            println!("decode_into_stream_string");
            e
        })
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

    pub fn ping(&self) -> impl Future<Item = String, Error = Error> {
        let url = "/_ping";

        let req = self.build_request2::<_, String, String>(
            &url,
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value2(req)
    }
}
