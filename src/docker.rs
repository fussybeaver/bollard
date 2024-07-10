use std::convert::Infallible;
#[cfg(feature = "ssl")]
use std::fs;
use std::future::Future;
#[cfg(feature = "ssl")]
use std::io;
use std::path::Path;
#[cfg(feature = "ssl")]
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use std::{cmp, env, fmt};

use futures_core::Stream;
use futures_util::future::FutureExt;
use futures_util::future::TryFutureExt;
use futures_util::stream::TryStreamExt;
use futures_util::StreamExt;
use http::header::CONTENT_TYPE;
use http::request::Builder;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::{self, body::Bytes, Method, Request, Response, StatusCode};
#[cfg(feature = "ssl")]
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
#[cfg(unix)]
use hyperlocal_next::UnixConnector;
use log::{debug, trace};
#[cfg(feature = "ssl")]
use rustls::{crypto::ring::sign::any_supported_type, sign::CertifiedKey, ALL_VERSIONS};
#[cfg(feature = "ssl")]
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use serde_derive::{Deserialize, Serialize};
use tokio::io::{split, AsyncRead, AsyncWrite};
use tokio_util::codec::FramedRead;

use crate::container::LogOutput;
use crate::errors::Error;
use crate::errors::Error::*;
use crate::read::{
    AsyncUpgraded, IncomingStream, JsonLineDecoder, NewlineLogOutputDecoder, StreamReader,
};
use crate::uri::Uri;
#[cfg(windows)]
use hyper_named_pipe::NamedPipeConnector;

use serde::de::DeserializeOwned;
use serde::ser::Serialize;

/// The default `DOCKER_SOCKET` address that we will try to connect to.
#[cfg(unix)]
pub const DEFAULT_SOCKET: &str = "unix:///var/run/docker.sock";

/// The default `DOCKER_NAMED_PIPE` address that a windows client will try to connect to.
#[cfg(windows)]
pub const DEFAULT_NAMED_PIPE: &str = "npipe:////./pipe/docker_engine";

/// The default `DOCKER_TCP_ADDRESS` address that we will try to connect to.
pub const DEFAULT_TCP_ADDRESS: &str = "tcp://localhost:2375";

/// The default `DOCKER_HOST` address that we will try to connect to.
#[cfg(unix)]
pub const DEFAULT_DOCKER_HOST: &str = DEFAULT_SOCKET;

/// The default `DOCKER_HOST` address that we will try to connect to.
#[cfg(windows)]
pub const DEFAULT_DOCKER_HOST: &str = DEFAULT_NAMED_PIPE;

/// Default timeout for all requests is 2 minutes.
const DEFAULT_TIMEOUT: u64 = 120;

/// Default Client Version to communicate with the server.
pub const API_DEFAULT_VERSION: &ClientVersion = &ClientVersion {
    major_version: 1,
    minor_version: 44,
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
        client: Client<HttpConnector, BodyType>,
    },
    #[cfg(feature = "ssl")]
    Https {
        client: Client<HttpsConnector<HttpConnector>, BodyType>,
    },
    #[cfg(unix)]
    Unix {
        client: Client<UnixConnector, BodyType>,
    },
    #[cfg(windows)]
    NamedPipe {
        client: Client<NamedPipeConnector, BodyType>,
    },
    #[cfg(test)]
    Mock {
        client: Client<yup_hyper_mock::HostToReplyConnector, BodyType>,
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
            #[cfg(test)]
            Transport::Mock { .. } => write!(f, "Mock"),
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
        &serde_json::to_string(t).map_err(|e| serde::ser::Error::custom(format!("{e}")))?,
    )
}

pub(crate) fn serialize_join_newlines<S>(t: &[&str], s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(&t.join("\n"))
}

#[cfg(feature = "time")]
pub fn deserialize_rfc3339<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<time::OffsetDateTime, D::Error> {
    let s: String = serde::Deserialize::deserialize(d)?;
    time::OffsetDateTime::parse(&s, &time::format_description::well_known::Rfc3339)
        .map_err(|e| serde::de::Error::custom(format!("{:?}", e)))
}

#[cfg(feature = "time")]
pub fn serialize_rfc3339<S: serde::Serializer>(
    date: &time::OffsetDateTime,
    s: S,
) -> Result<S::Ok, S::Error> {
    s.serialize_str(
        &date
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| serde::ser::Error::custom(format!("{:?}", e)))?,
    )
}

#[cfg(feature = "time")]
pub(crate) fn serialize_as_timestamp<S>(
    opt: &Option<crate::models::BollardDate>,
    s: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match opt {
        Some(t) => s.serialize_str(&format!(
            "{}.{}",
            t.unix_timestamp(),
            t.unix_timestamp_nanos()
        )),
        None => s.serialize_str(""),
    }
}

#[cfg(all(feature = "chrono", not(feature = "time")))]
pub(crate) fn serialize_as_timestamp<S>(
    opt: &Option<crate::models::BollardDate>,
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

/// Internal model: Docker Server JSON payload when an error is emitted
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct DockerServerErrorMessage {
    message: String,
}

#[cfg(feature = "ssl")]
#[derive(Debug)]
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
            let home = home::home_dir().ok_or_else(|| NoHomePathError)?;
            Ok(home.join(".docker"))
        }
    }

    fn open_buffered(path: &Path) -> Result<io::BufReader<fs::File>, Error> {
        Ok(io::BufReader::new(fs::File::open(path)?))
    }

    fn certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, Error> {
        Ok(rustls_pemfile::certs(&mut Self::open_buffered(path)?)
            .collect::<Result<Vec<CertificateDer<'static>>, io::Error>>()?)
    }

    fn keys(path: &Path) -> Result<Vec<PrivateKeyDer<'static>>, Error> {
        let mut rdr = Self::open_buffered(path)?;
        let mut keys = vec![];
        if let Some(key) = rustls_pemfile::private_key(&mut rdr).map_err(|_| CertPathError {
            path: path.to_path_buf(),
        })? {
            keys.push(key);
        }

        Ok(keys)
    }

    fn docker_client_key(&self) -> Result<Arc<CertifiedKey>, Error> {
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

        let signing_key = any_supported_type(&key).map_err(|_| CertParseError {
            path: self.ssl_key.to_owned(),
        })?;

        Ok(Arc::new(CertifiedKey::new(all_certs, signing_key)))
    }
}

#[cfg(feature = "ssl")]
impl rustls::client::ResolvesClientCert for DockerClientCertResolver {
    fn resolve(&self, _: &[&[u8]], _: &[rustls::SignatureScheme]) -> Option<Arc<CertifiedKey>> {
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
        Docker::connect_with_ssl(
            if let Ok(ref host) = env::var("DOCKER_HOST") {
                host
            } else {
                DEFAULT_TCP_ADDRESS
            },
            &cert_path.join("key.pem"),
            &cert_path.join("cert.pem"),
            &cert_path.join("ca.pem"),
            DEFAULT_TIMEOUT,
            API_DEFAULT_VERSION,
        )
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

        let mut root_store = rustls::RootCertStore::empty();

        #[cfg(not(feature = "webpki"))]
        for cert in rustls_native_certs::load_native_certs()? {
            root_store
                .add(cert)
                .map_err(|err| NoNativeCertsError { err })?
        }
        #[cfg(feature = "webpki")]
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let mut ca_pem = io::Cursor::new(fs::read(ssl_ca).map_err(|_| CertPathError {
            path: ssl_ca.to_owned(),
        })?);

        root_store.add_parsable_certificates(
            rustls_pemfile::certs(&mut ca_pem).collect::<Result<Vec<_>, _>>()?,
        );

        let config = rustls::ClientConfig::builder_with_provider(
            rustls::crypto::aws_lc_rs::default_provider().into(),
        )
        .with_protocol_versions(ALL_VERSIONS)?
        .with_root_certificates(root_store)
        .with_client_cert_resolver(Arc::new(DockerClientCertResolver {
            ssl_key: ssl_key.to_owned(),
            ssl_cert: ssl_cert.to_owned(),
        }));

        let mut http_connector = HttpConnector::new();
        http_connector.enforce_http(false);

        let https_connector: HttpsConnector<HttpConnector> =
            HttpsConnector::from((http_connector, config));

        let mut client_builder = Client::builder(TokioExecutor::new());
        client_builder.pool_max_idle_per_host(0);

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
        let host = env::var("DOCKER_HOST").unwrap_or_else(|_| DEFAULT_TCP_ADDRESS.to_string());
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

        let mut client_builder = Client::builder(TokioExecutor::new());
        client_builder.pool_max_idle_per_host(0);

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

    /// Connect using to either a Unix socket or a Windows named pipe using defaults common to the
    /// standard docker configuration.
    ///
    /// # Defaults
    ///
    ///  - The unix socket location defaults to `/var/run/docker.sock`. The windows named pipe
    ///  location defaults to `//./pipe/docker_engine`.
    ///  - The request timeout defaults to 2 minutes.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bollard::Docker;
    ///
    /// use futures_util::future::TryFutureExt;
    ///
    /// let connection = Docker::connect_with_socket_defaults().unwrap();
    /// connection.ping().map_ok(|_| Ok::<_, ()>(println!("Connected!")));
    /// ```
    pub fn connect_with_socket_defaults() -> Result<Docker, Error> {
        #[cfg(unix)]
        let path = DEFAULT_SOCKET;
        #[cfg(windows)]
        let path = DEFAULT_NAMED_PIPE;

        Docker::connect_with_socket(path, DEFAULT_TIMEOUT, API_DEFAULT_VERSION)
    }

    /// Connect using a Unix socket or a Windows named pipe.
    ///
    /// # Arguments
    ///
    ///  - `path`: connection unix socket path or windows named pipe path.
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
    /// let connection = Docker::connect_with_socket("/var/run/docker.sock", 120, API_DEFAULT_VERSION).unwrap();
    /// connection.ping().map_ok(|_| Ok::<_, ()>(println!("Connected!")));
    /// ```
    pub fn connect_with_socket(
        path: &str,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        // Remove the scheme if present
        let clean_path = path
            .trim_start_matches("unix://")
            .trim_start_matches("npipe://");

        // Check if the socket file exists
        if !std::path::Path::new(clean_path).exists() {
            return Err(Error::SocketNotFoundError(clean_path.to_string()));
        }

        #[cfg(unix)]
        let docker = Docker::connect_with_unix(path, timeout, client_version)?;
        #[cfg(windows)]
        let docker = Docker::connect_with_named_pipe(path, timeout, client_version)?;

        Ok(docker)
    }

    /// Connect using a Unix socket, a Windows named pipe, or via HTTP.
    /// The connection method is determined by the `DOCKER_HOST` environment variable.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bollard::{API_DEFAULT_VERSION, Docker};
    ///
    /// use futures_util::future::TryFutureExt;
    ///
    /// let connection = Docker::connect_with_defaults().unwrap();
    /// connection.ping().map_ok(|_| Ok::<_, ()>(println!("Connected!")));
    /// ```
    pub fn connect_with_defaults() -> Result<Docker, Error> {
        let host = env::var("DOCKER_HOST").unwrap_or_else(|_| DEFAULT_DOCKER_HOST.to_string());
        match host {
            #[cfg(unix)]
            h if h.starts_with("unix://") => {
                Docker::connect_with_unix(&h, DEFAULT_TIMEOUT, API_DEFAULT_VERSION)
            }
            #[cfg(windows)]
            h if h.starts_with("npipe://") => {
                Docker::connect_with_named_pipe(&h, DEFAULT_TIMEOUT, API_DEFAULT_VERSION)
            }
            h if h.starts_with("tcp://") || h.starts_with("http://") => {
                #[cfg(feature = "ssl")]
                if env::var("DOCKER_TLS_VERIFY").is_ok() {
                    return Docker::connect_with_ssl_defaults();
                }
                Docker::connect_with_http_defaults()
            }
            #[cfg(feature = "ssl")]
            h if h.starts_with("https://") => Docker::connect_with_ssl_defaults(),
            _ => Err(UnsupportedURISchemeError {
                uri: host.to_string(),
            }),
        }
    }
}

#[cfg(unix)]
/// A Docker implementation typed to connect to a Unix socket.
impl Docker {
    /// Connect using a Unix socket using defaults common to the standard docker configuration.
    ///
    /// # Defaults
    ///
    ///  - The socket location defaults to the value of `DEFAULT_SOCKET` env if its set and the URL
    ///    has `unix` scheme; otherwise `/var/run/docker.sock`.
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
        // Using 3 variables to not have to copy/allocate `DEFAULT_SOCKET`.
        let socket_path = env::var("DOCKER_HOST").ok().and_then(|p| {
            if p.starts_with("unix://") {
                Some(p)
            } else {
                None
            }
        });
        let path = socket_path.as_deref();
        let path_ref: &str = path.unwrap_or(DEFAULT_SOCKET);
        Docker::connect_with_unix(path_ref, DEFAULT_TIMEOUT, API_DEFAULT_VERSION)
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
        path: &str,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        let client_addr = path.replacen("unix://", "", 1);

        // check if the socket file exists and is accessible
        if !Path::new(&client_addr).exists() {
            return Err(Error::SocketNotFoundError(client_addr));
        }

        let unix_connector = UnixConnector;

        let mut client_builder = Client::builder(TokioExecutor::new());
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
    /// Connect using a Windows Named Pipe using defaults that are common to the standard docker
    /// configuration.
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
        path: &str,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        let client_addr = path.replacen("npipe://", "", 1);

        let named_pipe_connector = NamedPipeConnector;

        let mut client_builder = Client::builder(TokioExecutor::new());
        client_builder.http1_title_case_headers(true);
        client_builder.pool_max_idle_per_host(0);

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

#[cfg(test)]
impl Docker {
    ///
    ///  - `connector`: a `HostToReplyConnector` as defined in `yup_hyper_mock`
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
    ///   String::from("http://127.0.0.1"),
    ///   "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
    /// );
    /// let connection = Docker::connect_with_mock(connector, "127.0.0.1".to_string(), 5, API_DEFAULT_VERSION).unwrap();
    /// connection.ping()
    ///   .and_then(|_| Ok(println!("Connected!")));
    /// # }
    /// ```
    pub fn connect_with_mock(
        connector: yup_hyper_mock::HostToReplyConnector,
        client_addr: String,
        timeout: u64,
        client_version: &ClientVersion,
    ) -> Result<Docker, Error> {
        let client_builder = Client::builder(TokioExecutor::new());
        let client = client_builder.build(connector);

        let (transport, client_type) = (Transport::Mock { client }, ClientType::Http);

        let docker = Docker {
            transport: Arc::new(transport),
            client_type,
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

impl Docker {
    /// Set the request timeout.
    ///
    /// This timeout is shared by all requests to the Docker Engine API.
    ///
    /// By default, 2 minutes.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.set_timeout(timeout);
        self
    }

    /// Get the current timeout.
    ///
    /// This timeout is shared by all requests to the Docker Engine API.
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.client_timeout)
    }

    /// Set the request timeout.
    ///
    /// This timeout is shared by all requests to the Docker Engine API.
    ///
    /// By default, 2 minutes.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.client_timeout = timeout.as_secs();
    }
}

// The implementation block for Docker requests
impl Docker {
    pub(crate) fn process_into_value<T>(
        &self,
        req: Result<Request<BodyType>, Error>,
    ) -> impl Future<Output = Result<T, Error>>
    where
        T: DeserializeOwned,
    {
        let fut = self.process_request(req);
        async move { Docker::decode_response(fut.await?).await }
    }

    pub(crate) fn process_into_stream<T>(
        &self,
        req: Result<Request<BodyType>, Error>,
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
        req: Result<Request<BodyType>, Error>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> + Unpin {
        Box::pin(
            self.process_request(req)
                .map_ok(Docker::decode_into_stream_string)
                .try_flatten_stream(),
        )
    }

    pub(crate) fn process_into_unit(
        &self,
        req: Result<Request<BodyType>, Error>,
    ) -> impl Future<Output = Result<(), Error>> {
        let fut = self.process_request(req);
        async move {
            fut.await?;
            Ok(())
        }
    }

    pub(crate) fn process_into_body(
        &self,
        req: Result<Request<BodyType>, Error>,
    ) -> impl Stream<Item = Result<Bytes, Error>> + Unpin {
        Box::pin(
            self.process_request(req)
                .map_ok(|response| IncomingStream::new(response.into_body()))
                .into_stream()
                .try_flatten(),
        )
    }

    pub(crate) fn process_into_string(
        &self,
        req: Result<Request<BodyType>, Error>,
    ) -> impl Future<Output = Result<String, Error>> {
        let fut = self.process_request(req);
        async move {
            let response = fut.await?;
            Docker::decode_into_string(response).await
        }
    }

    pub(crate) async fn process_upgraded(
        &self,
        req: Result<Request<BodyType>, Error>,
    ) -> Result<(impl AsyncRead, impl AsyncWrite), Error> {
        let res = self.process_request(req).await?;
        let upgraded = hyper::upgrade::on(res).await?;
        let tokio_upgraded = AsyncUpgraded::new(upgraded);

        Ok(split(tokio_upgraded))
    }

    pub(crate) fn serialize_payload<S>(body: Option<S>) -> Result<BodyType, Error>
    where
        S: Serialize,
    {
        match body.map(|inst| serde_json::to_string(&inst)) {
            Some(Ok(res)) => Ok(Some(res)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
        .map(|payload| {
            debug!("{}", payload.clone().unwrap_or_default());
            payload
                .map(|content| BodyType::Left(Full::new(content.into())))
                .unwrap_or(BodyType::Left(Full::new(Bytes::new())))
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
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        let res = self
            .process_into_value::<crate::system::Version>(req)
            .await?;

        let server_version: ClientVersion = if let Some(api_version) = res.api_version {
            match api_version.into() {
                MaybeClientVersion::Some(client_version) => client_version,
                MaybeClientVersion::None => {
                    return Err(APIVersionParseError {});
                }
            }
        } else {
            return Err(APIVersionParseError {});
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

    pub(crate) fn process_request(
        &self,
        request: Result<Request<BodyType>, Error>,
    ) -> impl Future<Output = Result<Response<Incoming>, Error>> {
        let transport = self.transport.clone();
        let timeout = self.client_timeout;

        match request.as_ref().map(|b| b.body()) {
            Ok(http_body_util::Either::Left(bytes)) => trace!("request: {:?}", bytes),
            Ok(http_body_util::Either::Right(_)) => trace!("request: (stream)"),
            Err(e) => trace!("request: Err({e:?}"),
        };

        async move {
            let request = request?;
            let response = Docker::execute_request(transport, request, timeout).await?;

            let status = response.status();
            match status {
                // Status code 200 - 299 or 304
                s if s.is_success() || s == StatusCode::NOT_MODIFIED => Ok(response),

                StatusCode::SWITCHING_PROTOCOLS => Ok(response),

                // All other status codes
                _ => {
                    let contents = Docker::decode_into_string(response).await?;

                    let mut message = String::new();
                    if !contents.is_empty() {
                        message = serde_json::from_str::<DockerServerErrorMessage>(&contents)
                            .map(|msg| msg.message)
                            .or_else(|e| {
                                if e.is_data() || e.is_syntax() {
                                    Ok(contents)
                                } else {
                                    Err(e)
                                }
                            })?;
                    }
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
        payload: Result<BodyType, Error>,
    ) -> Result<Request<BodyType>, Error>
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
        let request_uri: hyper::Uri = uri.try_into()?;
        debug!("{}", &request_uri);
        Ok(builder
            .uri(request_uri)
            .header(CONTENT_TYPE, "application/json")
            .body(payload?)?)
    }

    async fn execute_request(
        transport: Arc<Transport>,
        req: Request<BodyType>,
        timeout: u64,
    ) -> Result<Response<Incoming>, Error> {
        // This is where we determine to which transport we issue the request.
        let request = match *transport {
            Transport::Http { ref client } => client.request(req),
            #[cfg(feature = "ssl")]
            Transport::Https { ref client } => client.request(req),
            #[cfg(unix)]
            Transport::Unix { ref client } => client.request(req),
            #[cfg(windows)]
            Transport::NamedPipe { ref client } => client.request(req),
            #[cfg(test)]
            Transport::Mock { ref client } => client.request(req),
        };

        match tokio::time::timeout(Duration::from_secs(timeout), request).await {
            Ok(v) => Ok(v?),
            Err(_) => Err(RequestTimeoutError),
        }
    }

    fn decode_into_stream<T>(res: Response<Incoming>) -> impl Stream<Item = Result<T, Error>>
    where
        T: DeserializeOwned,
    {
        FramedRead::new(StreamReader::new(res.into_body()), JsonLineDecoder::new())
    }

    fn decode_into_stream_string(
        res: Response<Incoming>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> {
        FramedRead::new(
            StreamReader::new(res.into_body()),
            NewlineLogOutputDecoder::new(false),
        )
        .map_err(Error::from)
    }

    async fn decode_into_string(response: Response<Incoming>) -> Result<String, Error> {
        let body = response.into_body().collect().await?.to_bytes();

        Ok(String::from_utf8_lossy(&body).to_string())
    }

    async fn decode_response<T>(response: Response<Incoming>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let bytes = response.into_body().collect().await?.to_bytes();

        debug!("Decoded into string: {}", &String::from_utf8_lossy(&bytes));

        serde_json::from_slice::<T>(&bytes).map_err(|e| {
            if e.is_data() {
                JsonDataError {
                    message: e.to_string(),
                    column: e.column(),
                    #[cfg(feature = "json_data_content")]
                    contents: String::from_utf8_lossy(&bytes).to_string(),
                }
            } else {
                e.into()
            }
        })
    }
}

pub(crate) type BodyType = http_body_util::Either<
    Full<Bytes>,
    StreamBody<Pin<Box<dyn Stream<Item = Result<Frame<Bytes>, Infallible>> + Send>>>,
>;

pub(crate) fn body_stream(body: impl Stream<Item = Bytes> + Send + 'static) -> BodyType {
    BodyType::Right(StreamBody::new(Box::pin(body.map(|a| Ok(Frame::data(a))))))
}
