use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::str::from_utf8;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dirs;
use failure::Error;
use futures::future::{self, result};
use futures::Stream;
use http::header::CONTENT_TYPE;
use http::request::Builder;
use hyper::client::connect::Connect;
use hyper::client::HttpConnector;
use hyper::rt::Future;
use hyper::{self, Body, Chunk, Client, Request, Response, StatusCode};
#[cfg(feature = "openssl")]
use hyper_openssl::HttpsConnector;
use hyper_tls;
#[cfg(unix)]
use hyperlocal::UnixConnector;
use native_tls::{Certificate, Identity, TlsConnector};
#[cfg(feature = "openssl")]
use openssl::ssl::SslConnector;
#[cfg(feature = "openssl")]
use openssl::ssl::{SslFiletype, SslMethod};
use tokio::timer::Timeout;
use tokio_codec::FramedRead;

use container::LogOutput;
use either::EitherResponse;
use errors::{
    DockerResponseBadParameterError, DockerResponseConflictError, DockerResponseNotFoundError,
    DockerResponseNotModifiedError, DockerResponseServerError, JsonDataError,
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

// Default number of threads for the connection pool, when using HTTP or HTTPS.
const DEFAULT_NUM_THREADS: usize = 1;

/// Default timeout for all requests is 2 minutes.
const DEFAULT_TIMEOUT: u64 = 120;

pub(crate) const TRUE_STR: &'static str = "true";
pub(crate) const FALSE_STR: &'static str = "false";

/// The default directory in which to look for our Docker certificate
/// files.
pub fn default_cert_path() -> Result<PathBuf, Error> {
    use errors::NoCertPathError;

    let from_env = env::var("DOCKER_CERT_PATH").or_else(|_| env::var("DOCKER_CONFIG"));
    if let Ok(ref path) = from_env {
        Ok(Path::new(path).to_owned())
    } else {
        let home = dirs::home_dir().ok_or_else(|| NoCertPathError {})?;
        Ok(home.join(".docker"))
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ClientType {
    #[cfg(unix)]
    Unix,
    Http,
    SSL,
    #[cfg(windows)]
    NamedPipe,
}

/// ---
///
/// # Docker
///
/// The main interface for calling the Docker API. Construct a new Docker instance using one of the
/// connect methods:
///  - [`Docker::connect_with_http_defaults`](struct.Docker.html#method.connect_with_http_defaults)
///  - [`Docker::connect_with_named_pipe_defaults`](struct.Docker.html#method.connect_with_pipe_defaults)
///  - [`Docker::connect_with_ssl_defaults`](struct.Docker.html#method.connect_with_ssl_defaults)
///  - [`Docker::connect_with_unix_defaults`](struct.Docker.html#method.connect_with_unix_defaults)
///  - [`Docker::connect_with_tls_defaults`](struct.Docker.html#method.connect_with_tls_defaults)
#[derive(Debug)]
pub struct Docker<C> {
    pub(crate) client: Arc<Client<C>>,
    pub(crate) client_type: ClientType,
    pub(crate) client_addr: String,
    pub(crate) client_timeout: u64,
}

impl<C> Clone for Docker<C> {
    fn clone(&self) -> Docker<C> {
        Docker {
            client: self.client.clone(),
            client_type: self.client_type.clone(),
            client_addr: self.client_addr.clone(),
            client_timeout: self.client_timeout,
        }
    }
}

#[cfg(feature = "openssl")]
/// A Docker implementation typed to connect to a secure HTTPS connection using the `openssl`
/// library.
impl Docker<HttpsConnector<HttpConnector>> {
    /// Connect using secure HTTPS using defaults that are signalled by environment variables.
    ///
    /// # Defaults
    ///
    ///  - The connection url is sourced from the `DOCKER_HOST` environment variable.
    ///  - The certificate directory is sourced from the `DOCKER_CERT_PATH` environment variable.
    ///  - Certificates are named `key.pem`, `cert.pem` and `ca.pem` to indicate the private key,
    ///  the server certificate and the certificate chain respectively.
    ///  - The number of threads used for the HTTP connection pool defaults to 1.
    ///  - The request timeout defaults to 2 minutes.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::Docker;
    ///
    /// use futures::future::Future;
    ///
    /// let connection = Docker::connect_with_ssl_defaults().unwrap();
    /// connection.ping()
    ///   .and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_ssl_defaults() -> Result<Docker<HttpsConnector<HttpConnector>>, Error> {
        let cert_path = default_cert_path()?;
        if let Ok(ref host) = env::var("DOCKER_HOST") {
            Docker::connect_with_ssl(
                host,
                &cert_path.join("key.pem"),
                &cert_path.join("cert.pem"),
                &cert_path.join("ca.pem"),
                DEFAULT_NUM_THREADS,
                DEFAULT_TIMEOUT,
            )
        } else {
            Docker::connect_with_ssl(
                DEFAULT_DOCKER_HOST,
                &cert_path.join("key.pem"),
                &cert_path.join("cert.pem"),
                &cert_path.join("ca.pem"),
                DEFAULT_NUM_THREADS,
                DEFAULT_TIMEOUT,
            )
        }
    }

    /// Connect using secure HTTPS.
    ///
    /// # Arguments
    ///
    ///  - `addr`: the connection url.
    ///  - `ssl_key`: the private key path.
    ///  - `ssl_cert`: the server certificate path.
    ///  - `ssl_ca`: the certificate chain path.
    ///  - `num_threads`: the number of threads for the HTTP connection pool.
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::Docker;
    ///
    /// use std::path::Path;
    ///
    /// use futures::future::Future;
    ///
    /// let connection = Docker::connect_with_ssl(
    ///     "localhost:2375",
    ///     Path::new("/certs/key.pem"),
    ///     Path::new("/certs/cert.pem"),
    ///     Path::new("/certs/ca.pem"),
    ///     1,
    ///     120).unwrap();
    /// connection.ping()
    ///   .and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_ssl(
        addr: &str,
        ssl_key: &Path,
        ssl_cert: &Path,
        ssl_ca: &Path,
        num_threads: usize,
        timeout: u64,
    ) -> Result<Docker<HttpsConnector<HttpConnector>>, Error> {
        // This ensures that using docker-machine-esque addresses work with Hyper.
        let client_addr = addr.replacen("tcp://", "", 1);

        let mut ssl_connector_builder = SslConnector::builder(SslMethod::tls())?;

        ssl_connector_builder.set_ca_file(ssl_ca)?;
        ssl_connector_builder.set_certificate_file(ssl_cert, SslFiletype::PEM)?;
        ssl_connector_builder.set_private_key_file(ssl_key, SslFiletype::PEM)?;

        let mut http_connector = HttpConnector::new(num_threads);
        http_connector.enforce_http(false);

        let https_connector: HttpsConnector<HttpConnector> =
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

/// A Docker implementation typed to connect to an unsecure Http connection.
impl Docker<HttpConnector> {
    /// Connect using unsecured HTTP using defaults that are signalled by environment variables.
    ///
    /// # Defaults
    ///
    ///  - The connection url is sourced from the `DOCKER_HOST` environment variable, and defaults
    ///  to `localhost:2375`.
    ///  - The number of threads used for the HTTP connection pool defaults to 1.
    ///  - The request timeout defaults to 2 minutes.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::Docker;
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
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::Docker;
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
        let client_addr = addr.replacen("tcp://", "", 1);

        let http_connector = HttpConnector::new(num_threads);

        let client_builder = Client::builder();
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
    /// Connect using a Unix socket using defaults that are signalled by environment variables.
    ///
    /// # Defaults
    ///
    ///  - The socket location defaults to `/var/run/docker.sock`.
    ///  - The request timeout defaults to 2 minutes.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::Docker;
    ///
    /// use futures::future::Future;
    ///
    /// let connection = Docker::connect_with_unix_defaults().unwrap();
    /// connection.ping().and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_unix_defaults() -> Result<Docker<impl Connect>, Error> {
        Docker::connect_with_unix(DEFAULT_SOCKET, DEFAULT_TIMEOUT)
    }

    /// Connect using a Unix socket.
    ///
    /// # Arguments
    ///
    ///  - `addr`: connection socket path.
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::Docker;
    ///
    /// use futures::future::Future;
    ///
    /// let connection = Docker::connect_with_unix("/var/run/docker.sock", 120).unwrap();
    /// connection.ping().and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_unix(addr: &str, timeout: u64) -> Result<Docker<impl Connect>, Error> {
        let client_addr = addr.replacen("unix://", "", 1);

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
/// A Docker implementation typed to connect to a Windows Named Pipe, exclusive to the windows
/// target.
impl Docker<NamedPipeConnector> {
    /// Connect using a Windows Named Pipe using defaults that are signalled by environment
    /// variables.
    ///
    /// # Defaults
    ///
    ///  - The socket location defaults to `//./pipe/docker_engine`.
    ///  - The request timeout defaults to 2 minutes.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::Docker;
    ///
    /// use futures::future::Future;
    ///
    /// let connection = Docker::connect_with_named_pipe_defaults().unwrap();
    /// connection.ping().and_then(|_| Ok(println!("Connected!")));
    ///
    /// # }
    /// ```
    pub fn connect_with_named_pipe_defaults() -> Result<Docker<NamedPipeConnector>, Error> {
        Docker::connect_with_named_pipe(DEFAULT_NAMED_PIPE, DEFAULT_TIMEOUT)
    }

    /// Connect using a Windows Named Pipe.
    ///
    /// # Arguments
    ///
    ///  - `addr`: socket location.
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::Docker;
    ///
    /// use futures::future::Future;
    ///
    ///
    /// let connection = Docker::connect_with_named_pipe(
    ///     "//./pipe/docker_engine", 120).unwrap();
    /// connection.ping().and_then(|_| Ok(println!("Connected!")));
    ///
    /// # }
    /// ```
    pub fn connect_with_named_pipe(
        addr: &str,
        timeout: u64,
    ) -> Result<Docker<NamedPipeConnector>, Error> {
        let client_addr = addr.replacen("npipe://", "", 1);

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

/// A Docker implementation typed to connect to a secure HTTPS connection, using the native rust
/// TLS library.
impl Docker<hyper_tls::HttpsConnector<HttpConnector>> {
    /// Connect using secure HTTPS using native TLS and defaults that are signalled by environment
    /// variables.
    ///
    /// # Defaults
    ///
    ///  - The connection url is sourced from the `DOCKER_HOST` environment variable.
    ///  - The certificate directory is sourced from the `DOCKER_CERT_PATH` environment variable.
    ///  - Certificate PKCS #12 archive is named `identity.pfx` and the certificate chain is named `ca.pem`.
    ///  - The password for the PKCS #12 archive defaults to an empty password.
    ///  - The number of threads used for the HTTP connection pool defaults to 1.
    ///  - The request timeout defaults to 2 minutes.
    ///
    ///  # PKCS12
    ///
    ///  PKCS #12 archives can be created with OpenSSL:
    ///
    ///  ```bash
    ///  openssl pkcs12 -export -out identity.pfx -inkey key.pem -in cert.pem -certfile
    ///  chain_certs.pem
    ///  ```
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::Docker;
    ///
    /// use futures::future::Future;
    ///
    /// let connection = Docker::connect_with_tls_defaults().unwrap();
    /// connection.ping()
    ///   .and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_tls_defaults(
) -> Result<Docker<hyper_tls::HttpsConnector<HttpConnector>>, Error> {
        let cert_path = default_cert_path()?;
        if let Ok(ref host) = env::var("DOCKER_HOST") {
            Docker::connect_with_tls(
                host,
                &cert_path.join("identity.pfx"),
                &cert_path.join("ca.pem"),
                "",
                DEFAULT_NUM_THREADS,
                DEFAULT_TIMEOUT,
            )
        } else {
            Docker::connect_with_tls(
                DEFAULT_DOCKER_HOST,
                &cert_path.join("identity.pfx"),
                &cert_path.join("ca.pem"),
                "",
                DEFAULT_NUM_THREADS,
                DEFAULT_TIMEOUT,
            )
        }
    }

    /// Connect using secure HTTPS using native TLS.
    ///
    /// # Arguments
    ///
    ///  - `addr`: the connection url.
    ///  - `pkcs12_file`: the PKCS #12 archive.
    ///  - `ca_file`: the certificate chain.
    ///  - `pkcs12_password`: the password to the PKCS #12 archive.
    ///  - `num_threads`: the number of threads for the HTTP connection pool.
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///
    ///  # PKCS12
    ///
    ///  PKCS #12 archives can be created with OpenSSL:
    ///
    ///  ```bash
    ///  openssl pkcs12 -export -out identity.pfx -inkey key.pem -in cert.pem -certfile
    ///  chain_certs.pem
    ///  ```
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::Docker;
    ///
    /// use std::path::Path;
    ///
    /// use futures::future::Future;
    ///
    /// let connection = Docker::connect_with_tls(
    ///     "localhost:2375",
    ///     Path::new("/certs/identity.pfx"),
    ///     Path::new("/certs/ca.pem"),
    ///     "my_secret_password",
    ///     1,
    ///     120
    /// ).unwrap();
    /// connection.ping()
    ///   .and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_tls(
        addr: &str,
        pkcs12_file: &Path,
        ca_file: &Path,
        pkcs12_password: &str,
        num_thread: usize,
        timeout: u64,
    ) -> Result<Docker<hyper_tls::HttpsConnector<HttpConnector>>, Error> {
        let client_addr = addr.replacen("tcp://", "", 1);

        let mut tls_connector_builder = TlsConnector::builder();

        let mut file = File::open(pkcs12_file)?;
        let mut buf = vec![];
        file.read_to_end(&mut buf)?;
        let identity = Identity::from_pkcs12(&buf, pkcs12_password)?;

        let mut file = File::open(ca_file)?;
        let mut buf = vec![];
        file.read_to_end(&mut buf)?;
        let ca = Certificate::from_pem(&buf)?;

        let tls_connector_builder = tls_connector_builder.identity(identity);
        tls_connector_builder.add_root_certificate(ca);

        let mut http_connector = HttpConnector::new(num_thread);
        http_connector.enforce_http(false);

        let https_connector: hyper_tls::HttpsConnector<HttpConnector> =
            hyper_tls::HttpsConnector::from((http_connector, tls_connector_builder.build()?));

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

impl<C> Docker<C>
where
    C: Connect + Sync + 'static,
{
    /// Connect using a type that implements `hyper::Connect`.
    ///
    /// # Arguments
    ///
    ///  - `connector`: type that implements `hyper::Connect`.
    ///  - `client_addr`: location to connect to.
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # extern crate yup_hyper_mock;
    /// # fn main () {
    /// use bollard::Docker;
    ///
    /// use futures::future::Future;
    ///
    /// # use yup_hyper_mock::SequentialConnector;
    /// let mut connector = SequentialConnector::default();
    /// connector.content.push(
    ///   "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
    /// );
    /// let connection = Docker::connect_with(connector, String::new(), 5).unwrap();
    /// connection.ping()
    ///   .and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with(
        connector: C,
        client_addr: String,
        timeout: u64,
    ) -> Result<Docker<C>, Error> {
        let client_builder = Client::builder();
        let client = client_builder.build(connector);

        #[cfg(unix)]
        let client_type = ClientType::Unix;
        #[cfg(windows)]
        let client_type = ClientType::NamedPipe;

        let docker = Docker {
            client: Arc::new(client),
            client_type: client_type,
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
/// use bollard::Docker;
/// let docker = Docker::connect_with_http_defaults().unwrap();
/// docker.chain();
/// ```
#[derive(Debug)]
pub struct DockerChain<C> {
    pub(super) inner: Docker<C>,
}

impl<C> Clone for DockerChain<C> {
    fn clone(&self) -> DockerChain<C> {
        DockerChain {
            inner: self.inner.clone(),
        }
    }
}

impl<C> Docker<C>
where
    C: Connect + Sync + 'static,
{
    /// Create a chain of docker commands, useful to calling the API in a sequential manner.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// use bollard::Docker;
    /// let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.chain();
    /// ```
    pub fn chain(self) -> DockerChain<C> {
        DockerChain { inner: self }
    }

    pub(crate) fn process_into_value<T>(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Future<Item = T, Error = Error>
    where
        T: DeserializeOwned,
    {
        self.process_request(req)
            .and_then(Docker::<C>::decode_response)
    }

    pub(crate) fn process_into_stream<T>(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Stream<Item = T, Error = Error>
    where
        T: DeserializeOwned,
    {
        self.process_request(req)
            .into_stream()
            .map(Docker::<C>::decode_into_stream::<T>)
            .flatten()
    }

    pub(crate) fn process_into_stream_string(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Stream<Item = LogOutput, Error = Error> {
        self.process_request(req)
            .into_stream()
            .map(Docker::<C>::decode_into_stream_string)
            .flatten()
    }

    pub(crate) fn process_into_unit(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Future<Item = (), Error = Error> {
        self.process_request(req).and_then(|_| Ok(()))
    }

    pub(crate) fn process_into_body(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Stream<Item = Chunk, Error = Error> {
        self.process_request(req)
            .into_stream()
            .map(|response| response.into_body().map_err(|e| e.into()))
            .flatten()
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
            debug!("{}", payload.clone().unwrap_or_else(String::new));
            payload
                .map(|content| content.into())
                .unwrap_or(Body::empty())
        })
    }

    fn process_request(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Future<Item = Response<Body>, Error = Error> {
        let client = self.client.clone();
        let timeout = self.client_timeout;

        result(req)
            .and_then(move |request| Docker::execute_request(client, request, timeout))
            .and_then(|response| {
                let status = response.status();
                match status {
                    // Status code 200 - 299
                    s if s.is_success() => EitherResponse::A(future::ok(response)),

                    // Status code 304: Not Modified
                    StatusCode::NOT_MODIFIED => {
                        EitherResponse::F(Docker::<C>::decode_into_string(response).and_then(
                            |message| Err(DockerResponseNotModifiedError { message }.into()),
                        ))
                    }

                    // Status code 409: Conflict
                    StatusCode::CONFLICT => {
                        EitherResponse::E(Docker::<C>::decode_into_string(response).and_then(
                            |message| Err(DockerResponseConflictError { message }.into()),
                        ))
                    }

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

    pub(crate) fn build_request<O, K, V>(
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
                let uri = Uri::parse(&self.client_addr, &self.client_type, path, q)?;
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
        timeout: u64,
    ) -> impl Future<Item = Response<Body>, Error = Error> {
        let now = Instant::now();

        Timeout::new_at(client.request(request), now + Duration::from_secs(timeout))
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
        ).map_err(|e| e)
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
            debug!("Decoded into string: {}", &contents);
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
}
