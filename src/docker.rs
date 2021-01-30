use std::cmp;
use std::env;
use std::fmt;
#[cfg(feature = "ssl")]
use std::fs;
use std::future::Future;
#[cfg(feature = "ssl")]
use std::io;
#[cfg(feature = "ssl")]
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_core::Stream;
use futures_util::future::FutureExt;
use futures_util::future::TryFutureExt;
use futures_util::stream;
use futures_util::stream::TryStreamExt;
use http::header::CONTENT_TYPE;
use http::request::Builder;
use hyper::client::{Client, HttpConnector};
use hyper::{self, body::Bytes, Body, Method, Request, Response, StatusCode};
#[cfg(feature = "ssl")]
use hyper_rustls::HttpsConnector;
#[cfg(unix)]
use hyperlocal::UnixClient as UnixConnector;
#[cfg(feature = "ssl")]
use rustls::internal::pemfile;
#[cfg(feature = "ssl")]
use rustls::sign::{CertifiedKey, RSASigningKey};
use tokio_util::codec::FramedRead;

use crate::container::LogOutput;
use crate::errors::Error;
use crate::errors::Error::*;
#[cfg(windows)]
use crate::named_pipe::NamedPipeConnector;
use crate::read::{JsonLineDecoder, NewlineLogOutputDecoder, StreamReader};
use crate::uri::Uri;

use serde::de::DeserializeOwned;
use serde::ser::Serialize;

/// The default `DOCKER_SOCKET` address that we will try to connect to.
#[cfg(unix)]
pub const DEFAULT_SOCKET: &str = "unix:///var/run/docker.sock";

/// The default `DOCKER_NAMED_PIPE` address that a windows client will try to connect to.
#[cfg(windows)]
pub const DEFAULT_NAMED_PIPE: &str = "npipe:////./pipe/docker_engine";

/// The default `DOCKER_HOST` address that we will try to connect to.
pub const DEFAULT_DOCKER_HOST: &str = "tcp://localhost:2375";

/// Default timeout for all requests is 2 minutes.
const DEFAULT_TIMEOUT: u64 = 120;

/// Default Client Version to communicate with the server.
pub const API_DEFAULT_VERSION: &ClientVersion = &ClientVersion {
    major_version: 1,
    minor_version: 40,
};

#[derive(Debug, Clone)]
pub(crate) enum ClientType {
    #[cfg(unix)]
    Unix,
    Http,
    #[cfg(feature = "ssl")]
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
    #[cfg(feature = "ssl")]
    Https {
        client: Client<HttpsConnector<HttpConnector>>,
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
            #[cfg(feature = "ssl")]
            Transport::Https { .. } => write!(f, "HTTPS(rustls)"),
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
/// See also [negotiate_version](Docker::negotiate_version()), and the `client_version` argument when instantiating the
/// [Docker] client instance.
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

impl<T: Into<String>> From<T> for MaybeClientVersion {
    fn from(s: T) -> MaybeClientVersion {
        match s
            .into()
            .split('.')
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

pub(crate) fn serialize_as_json<T, S>(t: &T, s: S) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: serde::Serializer,
{
    s.serialize_str(
        &serde_json::to_string(t).map_err(|e| serde::ser::Error::custom(format!("{}", e)))?,
    )
}

pub(crate) fn serialize_as_timestamp<S>(
    opt: &Option<DateTime<Utc>>,
    s: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match opt {
        Some(t) => s.serialize_str(&format!("{}.{}", t.timestamp(), t.timestamp_subsec_nanos())),
        None => s.serialize_str(""),
    }
}

#[derive(Debug)]
/// ---
///
/// # Docker
///
/// The main interface for calling the Docker API. Construct a new Docker instance using one of the
/// connect methods:
///  - [`Docker::connect_with_http_defaults`](Docker::connect_with_http_defaults())
///  - [`Docker::connect_with_named_pipe_defaults`](Docker::connect_with_named_pipe_defaults())
///  - [`Docker::connect_with_ssl_defaults`](Docker::connect_with_ssl_defaults())
///  - [`Docker::connect_with_unix_defaults`](Docker::connect_with_unix_defaults())
///  - [`Docker::connect_with_local_defaults`](Docker::connect_with_local_defaults())
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

#[cfg(feature = "ssl")]
struct DockerClientCertResolver {
    ssl_key: PathBuf,
    ssl_cert: PathBuf,
}

#[cfg(feature = "ssl")]
impl DockerClientCertResolver {
    /// The default directory in which to look for our Docker certificate
    /// files.
    pub fn default_cert_path() -> Result<PathBuf, Error> {
        let from_env = env::var("DOCKER_CERT_PATH").or_else(|_| env::var("DOCKER_CONFIG"));
        if let Ok(ref path) = from_env {
            Ok(Path::new(path).to_owned())
        } else {
            let home = dirs_next::home_dir().ok_or_else(|| NoCertPathError)?;
            Ok(home.join(".docker"))
        }
    }

    fn open_buffered(path: &Path) -> Result<io::BufReader<fs::File>, Error> {
        Ok(io::BufReader::new(fs::File::open(path)?))
    }

    fn certs(path: &Path) -> Result<Vec<rustls::Certificate>, Error> {
        Ok(
            pemfile::certs(&mut Self::open_buffered(path)?).map_err(|_| CertPathError {
                path: path.to_path_buf(),
            })?,
        )
    }

    fn keys(path: &Path) -> Result<Vec<rustls::PrivateKey>, Error> {
        let mut rdr = Self::open_buffered(path)?;
        let keys = pemfile::rsa_private_keys(&mut rdr).map_err(|_| CertPathError {
            path: path.to_path_buf(),
        })?;

        Ok(keys)
    }

    fn docker_client_key(&self) -> Result<CertifiedKey, Error> {
        let all_certs = Self::certs(&self.ssl_cert)?;

        let mut all_keys = Self::keys(&self.ssl_key)?;
        let key = if all_keys.len() == 1 {
            all_keys.remove(0)
        } else {
            return Err(CertMultipleKeys {
                count: all_keys.len(),
                path: self.ssl_key.to_owned(),
            });
        };

        let signing_key = RSASigningKey::new(&key).map_err(|_| CertParseError {
            path: self.ssl_key.to_owned(),
        })?;

        Ok(CertifiedKey::new(
            all_certs,
            Arc::new(Box::new(signing_key)),
        ))
    }
}

#[cfg(feature = "ssl")]
impl rustls::ResolvesClientCert for DockerClientCertResolver {
    fn resolve(&self, _: &[&[u8]], _: &[rustls::SignatureScheme]) -> Option<CertifiedKey> {
        self.docker_client_key().ok()
    }

    fn has_certs(&self) -> bool {
        true
    }
}

/// A Docker implementation typed to connect to a secure HTTPS connection using the `rustls`
/// library.
#[cfg(feature = "ssl")]
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
    /// use bollard::Docker;
    ///
    /// use futures_util::future::TryFutureExt;
    ///
    /// let connection = Docker::connect_with_ssl_defaults().unwrap();
    /// connection.ping()
    ///   .map_ok(|_| Ok::<_, ()>(println!("Connected!")));
    /// ```
    pub fn connect_with_ssl_defaults() -> Result<Docker, Error> {
        let cert_path = DockerClientCertResolver::default_cert_path()?;
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
    /// use bollard::{API_DEFAULT_VERSION, Docker};
    ///
    /// use std::path::Path;
    ///
    /// use futures_util::future::TryFutureExt;
    ///
    /// let connection = Docker::connect_with_ssl(
    ///     "tcp://localhost:2375/",
    ///     Path::new("/certs/key.pem"),
    ///     Path::new("/certs/cert.pem"),
    ///     Path::new("/certs/ca.pem"),
    ///     120,
    ///     API_DEFAULT_VERSION).unwrap();
    /// connection.ping()
    ///   .map_ok(|_| Ok::<_, ()>(println!("Connected!")));
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
        let client_addr = addr.replacen("tcp://", "", 1).replacen("https://", "", 1);

        let mut config = rustls::ClientConfig::new();
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        config.ct_logs = Some(&ct_logs::LOGS);

        config.root_store = match rustls_native_certs::load_native_certs() {
            Ok(store) => store,
            Err((Some(store), err)) => {
                warn!("could not load all certificates: {}", err);
                store
            }
            Err((None, err)) => {
                warn!("cannot access native certificate store: {}", err);
                config.root_store
            }
        };

        let mut ca_pem = io::Cursor::new(fs::read(ssl_ca)?);

        config
            .root_store
            .add_server_trust_anchors(&webpki_roots::TLS_SERVER_ROOTS);
        config
            .root_store
            .add_pem_file(&mut ca_pem)
            .map_err(|_| CertParseError {
                path: ssl_ca.to_owned(),
            })?;

        config.client_auth_cert_resolver = Arc::new(DockerClientCertResolver {
            ssl_key: ssl_key.to_owned(),
            ssl_cert: ssl_cert.to_owned(),
        });

        let mut http_connector = HttpConnector::new();
        http_connector.enforce_http(false);

        let https_connector: HttpsConnector<HttpConnector> =
            HttpsConnector::from((http_connector, config));

        let client_builder = Client::builder();
        let client = client_builder.build(https_connector);
        let transport = Transport::Https { client };
        let docker = Docker {
            transport: Arc::new(transport),
            client_type: ClientType::SSL,
            client_addr,
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
    /// use bollard::Docker;
    ///
    /// use futures_util::future::TryFutureExt;
    ///
    /// let connection = Docker::connect_with_http_defaults().unwrap();
    /// connection.ping()
    ///   .map_ok(|_| Ok::<_, ()>(println!("Connected!")));
    /// ```
    pub fn connect_with_http_defaults() -> Result<Docker, Error> {
        let host = env::var("DOCKER_HOST").unwrap_or_else(|_| DEFAULT_DOCKER_HOST.to_string());
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
    /// use bollard::{API_DEFAULT_VERSION, Docker};
    ///
    /// use futures_util::future::TryFutureExt;
    ///
    /// let connection = Docker::connect_with_http(
    ///                    "http://my-custom-docker-server:2735", 4, API_DEFAULT_VERSION)
    ///                    .unwrap();
    /// connection.ping()
    ///   .map_ok(|_| Ok::<_, ()>(println!("Connected!")));
    /// ```
    pub fn connect_with_http(
        addr: &str,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        // This ensures that using docker-machine-esque addresses work with Hyper.
        let client_addr = addr.replacen("tcp://", "", 1).replacen("http://", "", 1);

        let http_connector = HttpConnector::new();

        let client_builder = Client::builder();
        let client = client_builder.build(http_connector);
        let transport = Transport::Http { client };
        let docker = Docker {
            transport: Arc::new(transport),
            client_type: ClientType::Http,
            client_addr,
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
    /// use bollard::Docker;
    ///
    /// use futures_util::future::TryFutureExt;
    ///
    /// let connection = Docker::connect_with_unix_defaults().unwrap();
    /// connection.ping().map_ok(|_| Ok::<_, ()>(println!("Connected!")));
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
    /// use bollard::{API_DEFAULT_VERSION, Docker};
    ///
    /// use futures_util::future::TryFutureExt;
    ///
    /// let connection = Docker::connect_with_unix("/var/run/docker.sock", 120, API_DEFAULT_VERSION).unwrap();
    /// connection.ping().map_ok(|_| Ok::<_, ()>(println!("Connected!")));
    /// ```
    pub fn connect_with_unix(
        addr: &str,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        let client_addr = addr.replacen("unix://", "", 1);

        let unix_connector = UnixConnector;

        let mut client_builder = Client::builder();
        client_builder.pool_max_idle_per_host(0);

        let client = client_builder.build(unix_connector);
        let transport = Transport::Unix { client };
        let docker = Docker {
            transport: Arc::new(transport),
            client_type: ClientType::Unix,
            client_addr,
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
    /// use bollard::Docker;
    ///
    /// use futures_util::future::TryFutureExt;
    ///
    /// let connection = Docker::connect_with_named_pipe_defaults().unwrap();
    /// connection.ping().map_ok(|_| Ok::<_, ()>(println!("Connected!")));
    ///
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
    /// use bollard::{API_DEFAULT_VERSION, Docker};
    ///
    /// use futures_util::future::TryFutureExt;
    ///
    /// let connection = Docker::connect_with_named_pipe(
    ///     "//./pipe/docker_engine", 120, API_DEFAULT_VERSION).unwrap();
    /// connection.ping().map_ok(|_| Ok::<_, ()>(println!("Connected!")));
    ///
    /// ```
    pub fn connect_with_named_pipe(
        addr: &str,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        let client_addr = addr.replacen("npipe://", "", 1);

        let named_pipe_connector = NamedPipeConnector;

        let mut client_builder = Client::builder();
        client_builder.pool_max_idle_per_host(0);
        client_builder.http1_title_case_headers(true);
        let client = client_builder.build(named_pipe_connector);
        let transport = Transport::NamedPipe { client };
        let docker = Docker {
            transport: Arc::new(transport),
            client_type: ClientType::NamedPipe,
            client_addr,
            client_timeout: timeout,
            version: Arc::new((
                AtomicUsize::new(client_version.major_version),
                AtomicUsize::new(client_version.minor_version),
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
    /// [`Docker::connect_with_unix_defaults`]: Docker::connect_with_unix_defaults()
    /// [`Docker::connect_with_named_pipe_defaults`]: Docker::connect_with_named_pipe_defaults()
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
    /// [`Docker::connect_with_unix`]: Docker::connect_with_unix()
    /// [`Docker::connect_with_named_pipe`]: Docker::connect_with_named_pipe()
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

// The implementation block for Docker requests
impl Docker {
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
    ) -> impl Stream<Item = Result<Bytes, Error>> + Unpin {
        Box::pin(
            self.process_request(req)
                .map_ok(|response| response.into_body().map_err(Error::from))
                .into_stream()
                .try_flatten(),
        )
    }

    pub(crate) fn process_upgraded_stream_string(
        &self,
        req: Result<Request<Body>, Error>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> {
        let fut = self.process_request(req);
        stream::once(async move { fut.await.map(Docker::decode_into_upgraded_stream_string) })
            .try_flatten()
    }

    pub(crate) fn serialize_payload<S>(body: Option<S>) -> Result<Body, Error>
    where
        S: Serialize,
    {
        match body.map(|inst| serde_json::to_string(&inst)) {
            Some(Ok(res)) => Ok(Some(res)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
        .map(|payload| {
            debug!("{}", payload.clone().unwrap_or_else(String::new));
            payload
                .map(|content| content.into())
                .unwrap_or_else(Body::empty)
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
    /// ```rust,no_run
    ///     use bollard::Docker;
    ///
    ///     let docker = Docker::connect_with_http_defaults().unwrap();
    ///     async move {
    ///         &docker.negotiate_version().await.unwrap().version();
    ///     };
    /// ```
    pub async fn negotiate_version(self) -> Result<Self, Error> {
        let req = self.build_request(
            "/version",
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
        );

        let res = self
            .process_into_value::<crate::system::Version>(req)
            .await?;

        let err_api_version = res.api_version.as_ref().unwrap().clone();
        let server_version: ClientVersion = match res.api_version.as_ref().unwrap().into() {
            MaybeClientVersion::Some(client_version) => client_version,
            MaybeClientVersion::None => {
                return Err(APIVersionParseError {
                    api_version: err_api_version,
                })
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

        debug!("request: {:?}", request.as_ref().unwrap());

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
                    Err(DockerResponseNotModifiedError { message })
                }

                // Status code 409: Conflict
                StatusCode::CONFLICT => {
                    let message = Docker::decode_into_string(response).await?;
                    Err(DockerResponseConflictError { message })
                }

                // Status code 400: Bad request
                StatusCode::BAD_REQUEST => {
                    let message = Docker::decode_into_string(response).await?;
                    Err(DockerResponseBadParameterError { message })
                }

                // Status code 404: Not Found
                StatusCode::NOT_FOUND => {
                    let message = Docker::decode_into_string(response).await?;
                    Err(DockerResponseNotFoundError { message })
                }

                // All other status codes
                _ => {
                    let message = Docker::decode_into_string(response).await?;
                    Err(DockerResponseServerError {
                        status_code: status.as_u16(),
                        message,
                    })
                }
            }
        }
    }

    pub(crate) fn build_request<O>(
        &self,
        path: &str,
        builder: Builder,
        query: Option<O>,
        payload: Result<Body, Error>,
    ) -> Result<Request<Body>, Error>
    where
        O: Serialize,
    {
        let uri = Uri::parse(
            &self.client_addr,
            &self.client_type,
            path,
            query,
            &self.client_version(),
        )?;
        let request_uri: hyper::Uri = uri.into();
        debug!("{}", &request_uri);
        Ok(builder
            .uri(request_uri)
            .header(CONTENT_TYPE, "application/json")
            .body(payload?)?)
    }

    async fn execute_request(
        transport: Arc<Transport>,
        req: Request<Body>,
        timeout: u64,
    ) -> Result<Response<Body>, Error> {
        // This is where we determine to which transport we issue the request.
        let request = match *transport {
            Transport::Http { ref client } => client.request(req),
            #[cfg(feature = "ssl")]
            Transport::Https { ref client } => client.request(req),
            #[cfg(unix)]
            Transport::Unix { ref client } => client.request(req),
            #[cfg(windows)]
            Transport::NamedPipe { ref client } => client.request(req),
        };

        match tokio::time::timeout(Duration::from_secs(timeout), request).await {
            Ok(v) => Ok(v?),
            Err(_) => Err(RequestTimeoutError),
        }
    }

    fn decode_into_stream<T>(res: Response<Body>) -> impl Stream<Item = Result<T, Error>>
    where
        T: DeserializeOwned,
    {
        FramedRead::new(
            StreamReader::new(res.into_body().map_err(Error::from)),
            JsonLineDecoder::new(),
        )
    }

    fn decode_into_stream_string(
        res: Response<Body>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> {
        FramedRead::new(
            StreamReader::new(res.into_body().map_err(Error::from)),
            NewlineLogOutputDecoder::new(),
        )
    }

    fn decode_into_upgraded_stream_string(
        res: Response<Body>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> {
        hyper::upgrade::on(res)
            .into_stream()
            .map_ok(|r| FramedRead::new(r, NewlineLogOutputDecoder::new()))
            .try_flatten()
    }

    async fn decode_into_string(response: Response<Body>) -> Result<String, Error> {
        let body = hyper::body::to_bytes(response.into_body()).await?;

        Ok(String::from_utf8_lossy(&body).to_string())
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
            } else {
                e.into()
            }
        })
    }
}
