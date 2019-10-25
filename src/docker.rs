use std::cmp;
use std::env;
use std::fmt;
use std::future::Future;
#[cfg(any(feature = "ssl", feature = "tls"))]
use std::path::{Path, PathBuf};
use std::str::from_utf8;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

//use crate::hyper_mock::HostToReplyConnector;
use arrayvec::ArrayVec;
#[cfg(any(feature = "ssl", feature = "tls"))]
use dirs;
use futures_core::Stream;
use futures_util::future::FutureExt;
use futures_util::stream;
use futures_util::try_future::TryFutureExt;
use futures_util::try_stream::TryStreamExt;
use http::header::CONTENT_TYPE;
use http::request::Builder;
use hyper::client::HttpConnector;
use hyper::{self, Body, Chunk, Client, Method, Request, Response, StatusCode};
#[cfg(feature = "openssl")]
use hyper_openssl::HttpsConnector;
#[cfg(feature = "tls")]
use hyper_tls;
#[cfg(unix)]
use hyperlocal::UnixClient as UnixConnector;
#[cfg(feature = "tls")]
use native_tls::{Certificate, Identity, TlsConnector};
#[cfg(feature = "openssl")]
use openssl::ssl::SslConnector;
#[cfg(feature = "openssl")]
use openssl::ssl::{SslFiletype, SslMethod};
use tokio::timer::Timeout;
use tokio_codec::FramedRead;

use crate::container::LogOutput;
use crate::errors::Error;
use crate::errors::ErrorKind::{
    APIVersionParseError, DockerResponseBadParameterError, DockerResponseConflictError,
    DockerResponseNotFoundError, DockerResponseNotModifiedError, DockerResponseServerError,
    HttpClientError, HyperResponseError, JsonDataError, JsonDeserializeError, JsonSerializeError,
    RequestTimeoutError, StrParseError,
};
#[cfg(feature = "openssl")]
use crate::errors::ErrorKind::{NoCertPathError, SSLError};
use crate::read::{JsonLineDecoder, NewlineLogOutputDecoder, StreamReader};
use crate::system::Version;
use crate::uri::Uri;
#[cfg(windows)]
use named_pipe::NamedPipeConnector;

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

/// Default timeout for all requests is 2 minutes.
const DEFAULT_TIMEOUT: u64 = 120;

/// Default Client Version to communicate with the server.
pub const API_DEFAULT_VERSION: &'static ClientVersion = &ClientVersion {
    major_version: 1,
    minor_version: 40,
};

pub(crate) const TRUE_STR: &'static str = "true";
pub(crate) const FALSE_STR: &'static str = "false";

/// The default directory in which to look for our Docker certificate
/// files.
#[cfg(any(feature = "ssl", feature = "tls"))]
pub fn default_cert_path() -> Result<PathBuf, Error> {
    let from_env = env::var("DOCKER_CERT_PATH").or_else(|_| env::var("DOCKER_CONFIG"));
    if let Ok(ref path) = from_env {
        Ok(Path::new(path).to_owned())
    } else {
        let home = dirs::home_dir().ok_or_else(|| NoCertPathError)?;
        Ok(home.join(".docker"))
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ClientType {
    #[cfg(unix)]
    Unix,
    Http,
    #[cfg(any(feature = "ssl", feature = "tls"))]
    SSL,
    #[cfg(windows)]
    NamedPipe,
}

/// Transport is the type representing the means of communication
/// with the Docker daemon.
///
/// Each transport usually encapsulate a hyper client
/// with various Connect traits fulfilled.
pub(crate) enum Transport {
    Http {
        client: Client<HttpConnector>,
    },
    #[cfg(feature = "openssl")]
    Https {
        client: Client<HttpsConnector<HttpConnector>>,
    },
    #[cfg(feature = "tls")]
    Tls {
        client: Client<hyper_tls::HttpsConnector<HttpConnector>>,
    },
    #[cfg(unix)]
    Unix {
        client: Client<UnixConnector>,
    },
    #[cfg(windows)]
    NamedPipe {
        client: Client<NamedPipeConnector>,
    },
}

impl fmt::Debug for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Transport::Http { .. } => write!(f, "HTTP"),
            #[cfg(feature = "openssl")]
            Transport::Https { .. } => write!(f, "HTTPS(openssl)"),
            #[cfg(feature = "tls")]
            Transport::Tls { .. } => write!(f, "HTTPS(native)"),
            #[cfg(unix)]
            Transport::Unix { .. } => write!(f, "Unix"),
            #[cfg(windows)]
            Transport::NamedPipe { .. } => write!(f, "NamedPipe"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
/// Advisory version stub to use for communicating with the Server. The docker server will error if
/// a higher client version is used than is compatible with the server. Beware also, that the
/// docker server will return stubs for a higher version than the version set when communicating.
///
/// See also [negotiate_version](struct.Docker.html#method.negotiate_version), and the `client_version` argument when instantiating the
/// [Docker](struct.Docker.html) client instance.
pub struct ClientVersion {
    /// The major version number.
    pub major_version: usize,
    /// The minor version number.
    pub minor_version: usize,
}

pub(crate) enum MaybeClientVersion {
    Some(ClientVersion),
    None,
}

impl From<String> for MaybeClientVersion {
    fn from(s: String) -> MaybeClientVersion {
        match s
            .split(".")
            .map(|v| v.parse::<usize>())
            .collect::<Vec<Result<usize, std::num::ParseIntError>>>()
            .as_slice()
        {
            [Ok(first), Ok(second)] => MaybeClientVersion::Some(ClientVersion {
                major_version: first.to_owned(),
                minor_version: second.to_owned(),
            }),
            _ => MaybeClientVersion::None,
        }
    }
}

impl fmt::Display for ClientVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major_version, self.minor_version)
    }
}

impl PartialOrd for ClientVersion {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        match self.major_version.partial_cmp(&other.major_version) {
            Some(cmp::Ordering::Equal) => self.minor_version.partial_cmp(&other.minor_version),
            res => res,
        }
    }
}

impl From<&(AtomicUsize, AtomicUsize)> for ClientVersion {
    fn from(tpl: &(AtomicUsize, AtomicUsize)) -> ClientVersion {
        ClientVersion {
            major_version: tpl.0.load(Ordering::Relaxed),
            minor_version: tpl.1.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug)]
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
///  - [`Docker::connect_with_local_defaults`](struct.Docker.html#method.connect_with_local_defaults)
pub struct Docker {
    pub(crate) transport: Arc<Transport>,
    pub(crate) client_type: ClientType,
    pub(crate) client_addr: String,
    pub(crate) client_timeout: u64,
    pub(crate) version: Arc<(AtomicUsize, AtomicUsize)>,
}

impl Clone for Docker {
    fn clone(&self) -> Docker {
        Docker {
            transport: self.transport.clone(),
            client_type: self.client_type.clone(),
            client_addr: self.client_addr.clone(),
            client_timeout: self.client_timeout,
            version: self.version.clone(),
        }
    }
}

#[cfg(feature = "openssl")]
/// A Docker implementation typed to connect to a secure HTTPS connection using the `openssl`
/// library.
impl Docker {
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
    pub fn connect_with_ssl_defaults() -> Result<Docker, Error> {
        let cert_path = default_cert_path()?;
        if let Ok(ref host) = env::var("DOCKER_HOST") {
            Docker::connect_with_ssl(
                host,
                &cert_path.join("key.pem"),
                &cert_path.join("cert.pem"),
                &cert_path.join("ca.pem"),
                DEFAULT_TIMEOUT,
                API_DEFAULT_VERSION,
            )
        } else {
            Docker::connect_with_ssl(
                DEFAULT_DOCKER_HOST,
                &cert_path.join("key.pem"),
                &cert_path.join("cert.pem"),
                &cert_path.join("ca.pem"),
                DEFAULT_TIMEOUT,
                API_DEFAULT_VERSION,
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
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///  - `client_version`: the client version to communicate with the server.
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
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        // This ensures that using docker-machine-esque addresses work with Hyper.
        let client_addr = addr.replacen("tcp://", "", 1);

        let mut ssl_connector_builder = SslConnector::builder(SslMethod::tls())
            .map_err::<Error, _>(|e| SSLError { err: e }.into())?;

        ssl_connector_builder
            .set_ca_file(ssl_ca)
            .map_err::<Error, _>(|e| SSLError { err: e }.into())?;
        ssl_connector_builder
            .set_certificate_file(ssl_cert, SslFiletype::PEM)
            .map_err::<Error, _>(|e| SSLError { err: e }.into())?;
        ssl_connector_builder
            .set_private_key_file(ssl_key, SslFiletype::PEM)
            .map_err::<Error, _>(|e| SSLError { err: e }.into())?;

        let mut http_connector = HttpConnector::new();
        http_connector.enforce_http(false);

        let https_connector: HttpsConnector<HttpConnector> =
            HttpsConnector::with_connector(http_connector, ssl_connector_builder)
                .map_err::<Error, _>(|e| SSLError { err: e }.into())?;

        let client_builder = Client::builder();
        let client = client_builder.build(https_connector);
        let transport = Transport::Https { client };
        let docker = Docker {
            transport: Arc::new(transport),
            client_type: ClientType::SSL,
            client_addr: client_addr.to_owned(),
            client_timeout: timeout,
            version: Arc::new((
                AtomicUsize::new(client_version.major_version),
                AtomicUsize::new(client_version.minor_version),
            )),
        };

        Ok(docker)
    }
}

/// A Docker implementation typed to connect to an unsecure Http connection.
impl Docker {
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
    pub fn connect_with_http_defaults() -> Result<Docker, Error> {
        let host = env::var("DOCKER_HOST").unwrap_or(DEFAULT_DOCKER_HOST.to_string());
        Docker::connect_with_http(&host, DEFAULT_TIMEOUT, API_DEFAULT_VERSION)
    }

    /// Connect using unsecured HTTP.  
    ///
    /// # Arguments
    ///
    ///  - `addr`: connection url including scheme and port.
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///  - `client_version`: the client version to communicate with the server.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::{API_DEFAULT_VERSION, Docker};
    ///
    /// use futures::future::Future;
    ///
    /// let connection = Docker::connect_with_http(
    ///                    "http://my-custom-docker-server:2735", 4, 20, API_DEFAULT_VERSION)
    ///                    .unwrap();
    /// connection.ping()
    ///   .and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_http(
        addr: &str,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        // This ensures that using docker-machine-esque addresses work with Hyper.
        let client_addr = addr.replacen("tcp://", "", 1);

        let http_connector = HttpConnector::new();

        let client_builder = Client::builder();
        let client = client_builder.build(http_connector);
        let transport = Transport::Http { client };
        let docker = Docker {
            transport: Arc::new(transport),
            client_type: ClientType::Http,
            client_addr: client_addr.to_owned(),
            client_timeout: timeout,
            version: Arc::new((
                AtomicUsize::new(client_version.major_version),
                AtomicUsize::new(client_version.minor_version),
            )),
        };

        Ok(docker)
    }
}

#[cfg(unix)]
/// A Docker implementation typed to connect to a Unix socket.
impl Docker {
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
    pub fn connect_with_unix_defaults() -> Result<Docker, Error> {
        Docker::connect_with_unix(DEFAULT_SOCKET, DEFAULT_TIMEOUT, API_DEFAULT_VERSION)
    }

    /// Connect using a Unix socket.
    ///
    /// # Arguments
    ///
    ///  - `addr`: connection socket path.
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///  - `client_version`: the client version to communicate with the server.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::{API_DEFAULT_VERSION, Docker};
    ///
    /// use futures::future::Future;
    ///
    /// let connection = Docker::connect_with_unix("/var/run/docker.sock", 120, API_DEFAULT_VERSION).unwrap();
    /// connection.ping().and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_unix(
        addr: &str,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        let client_addr = addr.replacen("unix://", "", 1);

        let unix_connector = UnixConnector;

        let mut client_builder = Client::builder();
        client_builder.keep_alive(false);

        let client = client_builder.build(unix_connector);
        let transport = Transport::Unix { client };
        let docker = Docker {
            transport: Arc::new(transport),
            client_type: ClientType::Unix,
            client_addr: client_addr.to_owned(),
            client_timeout: timeout,
            version: Arc::new((
                AtomicUsize::new(client_version.major_version),
                AtomicUsize::new(client_version.minor_version),
            )),
        };

        Ok(docker)
    }
}

#[cfg(windows)]
/// A Docker implementation typed to connect to a Windows Named Pipe, exclusive to the windows
/// target.
impl Docker {
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
    pub fn connect_with_named_pipe_defaults() -> Result<Docker, Error> {
        Docker::connect_with_named_pipe(DEFAULT_NAMED_PIPE, DEFAULT_TIMEOUT, API_DEFAULT_VERSION)
    }

    /// Connect using a Windows Named Pipe.
    ///
    /// # Arguments
    ///
    ///  - `addr`: socket location.
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///  - `client_version`: the client version to communicate with the server.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    /// use bollard::{API_DEFAULT_VERSION, Docker};
    ///
    /// use futures::future::Future;
    ///
    ///
    /// let connection = Docker::connect_with_named_pipe(
    ///     "//./pipe/docker_engine", 120, API_DEFAULT_VERSION).unwrap();
    /// connection.ping().and_then(|_| Ok(println!("Connected!")));
    ///
    /// # }
    /// ```
    pub fn connect_with_named_pipe(
        addr: &str,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        let client_addr = addr.replacen("npipe://", "", 1);

        let named_pipe_connector = NamedPipeConnector::new();

        let mut client_builder = Client::builder();
        client_builder.keep_alive(false);
        client_builder.http1_title_case_headers(true);
        let client = client_builder.build(named_pipe_connector);
        let transport = Transport::NamedPipe { client };
        let docker = Docker {
            transport: Arc::new(transport),
            client_type: ClientType::NamedPipe,
            client_addr: client_addr.to_owned(),
            client_timeout: timeout,
            version: Arc::new((
                AtomicUsize::new(API_DEFAULT_VERSION.major_version),
                AtomicUsize::new(API_DEFAULT_VERSION.minor_version),
            )),
        };

        Ok(docker)
    }
}

/// A Docker implementation that wraps away which local implementation we are calling.
#[cfg(any(unix, windows))]
impl Docker {
    /// Connect using the local machine connection method with default arguments.
    ///
    /// This is a simple wrapper over the OS specific handlers:
    ///  * Unix: [`Docker::connect_with_unix_defaults`]
    ///  * Windows: [`Docker::connect_with_named_pipe_defaults`]
    ///
    /// [`Docker::connect_with_unix_defaults`]: struct.Docker.html#method.connect_with_unix_defaults
    /// [`Docker::connect_with_named_pipe_defaults`]: struct.Docker.html#method.connect_with_named_pipe_defaults
    pub fn connect_with_local_defaults() -> Result<Docker, Error> {
        #[cfg(unix)]
        return Docker::connect_with_unix_defaults();
        #[cfg(windows)]
        return Docker::connect_with_named_pipe_defaults();
    }

    /// Connect using the local machine connection method with supplied arguments.
    ///
    /// This is a simple wrapper over the OS specific handlers:
    ///  * Unix: [`Docker::connect_with_unix`]
    ///  * Windows: [`Docker::connect_with_named_pipe`]
    ///
    /// [`Docker::connect_with_unix`]: struct.Docker.html#method.connect_with_unix
    /// [`Docker::connect_with_named_pipe`]: struct.Docker.html#method.connect_with_named_pipe
    pub fn connect_with_local(
        addr: &str,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        #[cfg(unix)]
        return Docker::connect_with_unix(addr, timeout, client_version);
        #[cfg(windows)]
        return Docker::connect_with_named_pipe(addr, timeout, client_version);
    }
}

/// A Docker implementation typed to connect to a secure HTTPS connection, using the native rust
/// TLS library.
#[cfg(feature = "tls")]
impl Docker {
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
    pub fn connect_with_tls_defaults() -> Result<Docker, Error> {
        let cert_path = default_cert_path()?;
        if let Ok(ref host) = env::var("DOCKER_HOST") {
            Docker::connect_with_tls(
                host,
                &cert_path.join("identity.pfx"),
                &cert_path.join("ca.pem"),
                "",
                DEFAULT_TIMEOUT,
                API_DEFAULT_VERSION,
            )
        } else {
            Docker::connect_with_tls(
                DEFAULT_DOCKER_HOST,
                &cert_path.join("identity.pfx"),
                &cert_path.join("ca.pem"),
                "",
                DEFAULT_TIMEOUT,
                API_DEFAULT_VERSION,
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
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///  - `client_version`: the client version to communicate with the server.
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
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        let client_addr = addr.replacen("tcp://", "", 1);

        let mut tls_connector_builder = TlsConnector::builder();

        use crate::errors::ErrorKind;
        use std::fs::File;
        use std::io::Read;
        let mut file = File::open(pkcs12_file)?;
        let mut buf = vec![];
        file.read_to_end(&mut buf)?;
        let identity = Identity::from_pkcs12(&buf, pkcs12_password)
            .map_err(|err| ErrorKind::TLSError { err })?;

        let mut file = File::open(ca_file)?;
        let mut buf = vec![];
        file.read_to_end(&mut buf)?;
        let ca = Certificate::from_pem(&buf).map_err(|err| ErrorKind::TLSError { err })?;

        let tls_connector_builder = tls_connector_builder.identity(identity);
        tls_connector_builder.add_root_certificate(ca);

        let mut http_connector = HttpConnector::new();
        http_connector.enforce_http(false);

        let tls_connector = tls_connector_builder
            .build()
            .map_err(|err| ErrorKind::TLSError { err })?;
        let https_connector: hyper_tls::HttpsConnector<HttpConnector> =
            hyper_tls::HttpsConnector::from((http_connector, tls_connector.into()));

        let client_builder = Client::builder();
        let client = client_builder.build(https_connector);
        let transport = Transport::Tls { client };
        let docker = Docker {
            transport: Arc::new(transport),
            client_type: ClientType::SSL,
            client_addr: client_addr.to_owned(),
            client_timeout: timeout,
            version: Arc::new((
                AtomicUsize::new(client_version.major_version),
                AtomicUsize::new(client_version.minor_version),
            )),
        };

        Ok(docker)
    }
}

#[derive(Debug)]
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
pub struct DockerChain {
    pub(super) inner: Docker,
}

impl Clone for DockerChain {
    fn clone(&self) -> DockerChain {
        DockerChain {
            inner: self.inner.clone(),
        }
    }
}

// The implementation block for Docker requests
impl Docker {
    /// Create a chain of docker commands, useful to calling the API in a sequential manner.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// use bollard::Docker;
    /// let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.chain();
    /// ```
    pub fn chain(self) -> DockerChain {
        DockerChain { inner: self }
    }

    pub(crate) fn process_into_value<T>(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Future<Output = Result<T, Error>>
    where
        T: DeserializeOwned,
    {
        let fut = self.process_request(req);
        async move {
            let response = fut.await?;
            Docker::decode_response(response).await
        }
    }

    pub(crate) fn process_into_stream<T>(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Stream<Item = Result<T, Error>> + Unpin
    where
        T: DeserializeOwned,
    {
        Box::pin(
            self.process_request(req)
                .map_ok(Docker::decode_into_stream::<T>)
                .into_stream()
                .try_flatten(),
        )
    }

    pub(crate) fn process_into_stream_string(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> + Unpin {
        Box::pin(
            self.process_request(req)
                .map_ok(Docker::decode_into_stream_string)
                .try_flatten_stream(),
        )
    }

    pub(crate) fn process_into_unit(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Future<Output = Result<(), Error>> {
        let fut = self.process_request(req);
        async move {
            fut.await?;
            Ok(())
        }
    }

    pub(crate) fn process_into_body(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Stream<Item = Result<Chunk, Error>> + Unpin {
        Box::pin(
            self.process_request(req)
                .map_ok(|response| {
                    response
                        .into_body()
                        .map_err::<Error, _>(|e: hyper::Error| HyperResponseError { err: e }.into())
                })
                .into_stream()
                .try_flatten(),
        )
    }

    pub(crate) fn process_upgraded_stream_string<'a>(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> {
        let fut = self.process_request(req);
        stream::once(async move { fut.await.map(Docker::decode_into_upgraded_stream_string) })
            .try_flatten()
    }

    pub(crate) fn transpose_option<T>(
        option: Option<Result<T, Error>>,
    ) -> Result<Option<T>, Error> {
        option.transpose()
    }

    pub(crate) fn serialize_payload<S>(body: Option<S>) -> Result<Body, Error>
    where
        S: Serialize,
    {
        match body.map(|inst| serde_json::to_string(&inst)) {
            Some(Ok(res)) => Ok(Some(res)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
        .map_err(|e| JsonSerializeError { err: e }.into())
        .map(|payload| {
            debug!("{}", payload.clone().unwrap_or_else(String::new));
            payload
                .map(|content| content.into())
                .unwrap_or(Body::empty())
        })
    }

    /// Return the currently set client version.
    pub fn client_version(&self) -> ClientVersion {
        self.version.as_ref().into()
    }

    /// Check with the server for a supported version, and downgrade the client version if
    /// appropriate.
    ///
    /// # Examples:
    ///
    /// ```rust,norun
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # fn main () {
    ///     use bollard::Docker;
    ///
    ///     use futures::future::Future;
    ///
    ///     let docker = Docker::connect_with_http_defaults().unwrap();
    ///     docker.negotiate_version().map(|docker| {
    ///         docker.version()
    ///     });
    /// # }
    /// ```
    pub async fn negotiate_version(self) -> Result<Self, Error> {
        let req = self.build_request::<_, String, String>(
            "/version",
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        let res = self.process_into_value::<Version>(req).await?;

        let err_api_version = res.api_version.clone();
        let server_version: ClientVersion = match res.api_version.into() {
            MaybeClientVersion::Some(client_version) => client_version,
            MaybeClientVersion::None => {
                return Err(APIVersionParseError {
                    api_version: err_api_version,
                }
                .into())
            }
        };
        if server_version < self.client_version() {
            self.version
                .0
                .store(server_version.major_version, Ordering::Relaxed);
            self.version
                .1
                .store(server_version.minor_version, Ordering::Relaxed);
        }
        Ok(self)
    }

    fn process_request(
        &self,
        request: Result<Request<Body>, Error>,
    ) -> impl Future<Output = Result<Response<Body>, Error>> {
        let transport = self.transport.clone();
        let timeout = self.client_timeout;

        async move {
            let request = request?;
            let response = Docker::execute_request(transport, request, timeout).await?;

            let status = response.status();
            match status {
                // Status code 200 - 299
                s if s.is_success() => Ok(response),

                StatusCode::SWITCHING_PROTOCOLS => Ok(response),

                // Status code 304: Not Modified
                StatusCode::NOT_MODIFIED => {
                    let message = Docker::decode_into_string(response).await?;
                    Err(DockerResponseNotModifiedError { message }.into())
                }

                // Status code 409: Conflict
                StatusCode::CONFLICT => {
                    let message = Docker::decode_into_string(response).await?;
                    Err(DockerResponseConflictError { message }.into())
                }

                // Status code 400: Bad request
                StatusCode::BAD_REQUEST => {
                    let message = Docker::decode_into_string(response).await?;
                    Err(DockerResponseBadParameterError { message }.into())
                }

                // Status code 404: Not Found
                StatusCode::NOT_FOUND => {
                    let message = Docker::decode_into_string(response).await?;
                    Err(DockerResponseNotFoundError { message }.into())
                }

                // All other status codes
                _ => {
                    let message = Docker::decode_into_string(response).await?;
                    Err(DockerResponseServerError {
                        status_code: status.as_u16(),
                        message,
                    }
                    .into())
                }
            }
        }
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
                let uri = Uri::parse(
                    &self.client_addr,
                    &self.client_type,
                    path,
                    q,
                    &self.client_version(),
                )?;
                let request_uri: hyper::Uri = uri.into();
                Ok(builder
                    .uri(request_uri)
                    .header(CONTENT_TYPE, "application/json")
                    .body(body)
                    .map_err::<Error, _>(|e| {
                        HttpClientError {
                            builder: format!("{:?}", builder),
                            err: e,
                        }
                        .into()
                    })?)
            })
    }

    async fn execute_request(
        transport: Arc<Transport>,
        req: Request<Body>,
        timeout: u64,
    ) -> Result<Response<Body>, Error> {
        let now = Instant::now();

        // This is where we determine to which transport we issue the request.
        let request = match *transport {
            Transport::Http { ref client } => client.request(req),
            #[cfg(feature = "openssl")]
            Transport::Https { ref client } => client.request(req),
            #[cfg(feature = "tls")]
            Transport::Tls { ref client } => client.request(req),
            #[cfg(unix)]
            Transport::Unix { ref client } => client.request(req),
            #[cfg(windows)]
            Transport::NamedPipe { ref client } => client.request(req),
        };

        match Timeout::new_at(request, now + Duration::from_secs(timeout)).await {
            Ok(v) => v.map_err(|err| HyperResponseError { err }.into()),
            Err(_) => Err(RequestTimeoutError.into()),
        }
    }

    fn decode_into_stream<T>(res: Response<Body>) -> impl Stream<Item = Result<T, Error>>
    where
        T: DeserializeOwned,
    {
        FramedRead::new(
            StreamReader::new(
                res.into_body()
                    .map_err::<Error, _>(|e: hyper::Error| HyperResponseError { err: e }.into()),
            ),
            JsonLineDecoder::new(),
        )
    }

    fn decode_into_stream_string(
        res: Response<Body>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> {
        FramedRead::new(
            StreamReader::new(
                res.into_body()
                    .map_err::<Error, _>(|e: hyper::Error| HyperResponseError { err: e }.into()),
            ),
            NewlineLogOutputDecoder::new(),
        )
    }

    fn decode_into_upgraded_stream_string(
        res: Response<Body>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> {
        res.into_body()
            .on_upgrade()
            .into_stream()
            .map_ok(|r| FramedRead::new(r, NewlineLogOutputDecoder::new()))
            .map_err::<Error, _>(|e| HyperResponseError { err: e }.into())
            .try_flatten()
    }

    async fn decode_into_string(response: Response<Body>) -> Result<String, Error> {
        let body = response
            .into_body()
            .try_concat()
            .await
            .map_err(|e| HyperResponseError { err: e })?;

        from_utf8(&body).map(|x| x.to_owned()).map_err(|e| {
            StrParseError {
                content: hex::encode(body.to_owned()),
                err: e,
            }
            .into()
        })
    }

    async fn decode_response<T>(response: Response<Body>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let contents = Docker::decode_into_string(response).await?;

        debug!("Decoded into string: {}", &contents);
        serde_json::from_str::<T>(&contents).map_err(|e| {
            if e.is_data() {
                JsonDataError {
                    message: e.to_string(),
                    column: e.column(),
                    contents: contents.to_owned(),
                }
                .into()
            } else {
                JsonDeserializeError {
                    content: contents.to_owned(),
                    err: e,
                }
                .into()
            }
        })
    }

    /*
    /// Connect using the `HostToReplyConnector`.
    ///
    /// This connector is used to test the Docker client api.
    ///
    /// # Arguments
    ///
    ///  - `connector`: the HostToReplyConnector.
    ///  - `client_addr`: location to connect to.
    ///  - `timeout`: the read/write timeout (seconds) to use for every hyper connection
    ///  - `client_version`: the client version to communicate with the server.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # extern crate bollard;
    /// # extern crate futures;
    /// # extern crate yup_hyper_mock;
    /// # fn main () {
    /// use bollard::{API_DEFAULT_VERSION, Docker};
    ///
    /// use futures::future::Future;
    ///
    /// # use yup_hyper_mock::HostToReplyConnector;
    /// let mut connector = HostToReplyConnector::default();
    /// connector.m.insert(
    ///   format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
    ///   "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
    /// );
    /// let connection = Docker::connect_with_host_to_reply(connector, String::new(), 5, API_DEFAULT_VERSION).unwrap();
    /// connection.ping()
    ///   .and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_host_to_reply(
        connector: HostToReplyConnector,
        client_addr: String,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        let client_builder = Client::builder();
        let client = client_builder.build(connector);

        #[cfg(unix)]
        let client_type = ClientType::Unix;
        #[cfg(windows)]
        let client_type = ClientType::NamedPipe;
        let transport = Transport::HostToReply { client };

        let docker = Docker {
            transport: Arc::new(transport),
            client_type: client_type,
            client_addr,
            client_timeout: timeout,
            version: Arc::new((
                AtomicUsize::new(client_version.major_version),
                AtomicUsize::new(client_version.minor_version),
            )),
        };

        Ok(docker)
    }
    */
}
