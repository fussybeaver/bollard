//! Errors for this module.

#[cfg(feature = "ssl")]
use std::path::PathBuf;

/// The type of error embedded in an Error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error emitted during client instantiation when the `DOCKER_CERT_PATH` environment variable
    /// is invalid.
    #[cfg(feature = "ssl")]
    #[error("Could not find home directory")]
    NoHomePathError,
    /// Generic error when reading a certificate from the filesystem
    #[cfg(feature = "ssl")]
    #[error("Cannot open/read certificate with path: {path}")]
    CertPathError {
        /// Path for the failing certificate file
        path: PathBuf,
    },
    /// Error emitted when multiple keys are found in a certificate file
    #[cfg(feature = "ssl")]
    #[error("Found multiple keys ({count}), expected one: {path}")]
    CertMultipleKeys {
        /// Number of keys found in the certificate file
        count: usize,
        /// Path for the failing certificate file
        path: PathBuf,
    },
    /// Parse error for RSA encrypted keys
    #[cfg(feature = "ssl")]
    #[error("Could not parse key: {path}")]
    CertParseError {
        /// Path for the failing certificate file
        path: PathBuf,
    },
    /// Error emitted when the client is unable to load native certs for SSL
    #[cfg(feature = "ssl")]
    #[error("Could not load native certs")]
    NoNativeCertsError {
        /// The original error emitted.
        err: rustls::Error,
    },
    /// Generic error emitted by the docker server.
    #[error("Docker responded with status code {status_code}: {message}")]
    DockerResponseServerError {
        /// Status code returned by the docker server.
        status_code: u16,
        /// Message returned by the docker server.
        message: String,
    },
    /// Error facilitating debugging failed JSON parsing.
    #[error("Failed to deserialize JSON: {message}")]
    JsonDataError {
        /// Short section of the json close to the error.
        message: String,
        /// Entire JSON payload. This field is toggled with the **json_data_content** feature cargo flag.
        #[cfg(feature = "json_data_content")]
        contents: String,
        /// Character sequence at error location.
        column: usize,
    },
    /// Error emitted when the docker is requested to build with buildkit without a session id
    #[error("Failed to parse API version: {api_version}")]
    APIVersionParseError {
        /// The api version returned by the server.
        api_version: String,
    },
    /// Error emitted when a request times out.
    #[error("Timeout error")]
    RequestTimeoutError,
    /// Error emitted mid-stream as part of a successful docker operation
    #[error("Docker stream error")]
    DockerStreamError {
        /// error string emitted by the Stream
        error: String,
    },
    /// Error emitted as part of a container wait response
    #[error("Docker container wait error")]
    DockerContainerWaitError {
        /// error string returned from container wait call
        error: String,
        /// error code returned from container wait call
        code: i64,
    },
    /// Error emitted when a session is not provided to the buildkit engine
    #[error("Buildkit requires a unique session")]
    MissingSessionBuildkitError {},
    /// Error emitted when JSON fails to serialize.
    #[error(transparent)]
    JsonSerdeError {
        /// The original error emitted by serde.
        #[from]
        err: serde_json::Error,
    },
    /// Error emitted when log output generates an I/O error.
    #[error(transparent)]
    StrParseError {
        /// The original error emitted.
        #[from]
        err: std::str::Utf8Error,
    },
    /// Error emitted from an I/O error.
    #[error(transparent)]
    IOError {
        /// The original error emitted.
        #[from]
        err: std::io::Error,
    },
    /// Error emitted from a formatting error.
    #[error(transparent)]
    StrFmtError {
        /// The original error emitted.
        #[from]
        err: std::fmt::Error,
    },
    /// Error emitted from an HTTP error.
    #[error(transparent)]
    HttpClientError {
        /// The original error emitted.
        #[from]
        err: http::Error,
    },
    /// Error emitted from an HTTP error.
    #[error(transparent)]
    HyperResponseError {
        /// The original error emitted.
        #[from]
        err: hyper::Error,
    },
    /// Error emitted when serde fails to urlencod a struct of options
    #[error(transparent)]
    URLEncodedError {
        /// The original error emitted.
        #[from]
        err: serde_urlencoded::ser::Error,
    },
    #[cfg(feature = "buildkit")]
    /// Error emitted when a GRPC network request or response fails with docker's buildkit client
    #[error(transparent)]
    TonicError {
        /// The tonic error emitted.
        #[from]
        err: tonic::transport::Error,
    },
}
