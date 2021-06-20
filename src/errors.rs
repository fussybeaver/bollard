//! Errors for this module.
use std::path::PathBuf;

/// The type of error embedded in an Error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error emitted during client instantiation when the `DOCKER_CERT_PATH` environment variable
    /// is invalid.
    #[error("Could not find DOCKER_CERT_PATH")]
    NoCertPathError,
    /// Generic error when reading a certificate from the filesystem
    #[error("Cannot open/read certificate with path: {path}")]
    CertPathError {
        /// Path for the failing certificate file
        path: PathBuf,
    },
    /// Error emitted when multiple keys are found in a certificate file
    #[error("Found multiple keys ({count}), expected one: {path}")]
    CertMultipleKeys {
        /// Number of keys found in the certificate file
        count: usize,
        /// Path for the failing certificate file
        path: PathBuf,
    },
    /// Parse error for RSA encrypted keys
    #[error("Could not parse key: {path}")]
    CertParseError {
        /// Path for the failing certificate file
        path: PathBuf,
    },
    /// Error emitted by the docker server, when it responds with a 404.
    #[error("API responded with a 404 not found: {message}")]
    DockerResponseNotFoundError {
        /// Message returned by the docker server.
        message: String,
    },
    /// Generic error emitted by the docker server.
    #[error("Docker responded with status code {status_code}: {message}")]
    DockerResponseServerError {
        /// Status code returned by the docker server.
        status_code: u16,
        /// Message returned by the docker server.
        message: String,
    },
    /// Error emitted by the docker server, when it responds with a 400.
    #[error("API queried with a bad parameter: {message}")]
    DockerResponseBadParameterError {
        /// Message returned by the docker server.
        message: String,
    },
    /// Error emitted by the docker server, when it responds with a 409.
    #[error("API responded with a 409 conflict: {message}")]
    DockerResponseConflictError {
        /// Message returned by the docker server.
        message: String,
    },
    /// Error emitted by the docker server, when it responds with a 304.
    #[error("API responded with a 304, resource was not modified: {message}")]
    DockerResponseNotModifiedError {
        /// Message returned by the docker server.
        message: String,
    },
    /// Error facilitating debugging failed JSON parsing.
    #[error("Failed to deserialize JSON: {message}")]
    JsonDataError {
        /// Short section of the json close to the error.
        message: String,
        /// Entire JSON payload.
        contents: String,
        /// Character sequence at error location.
        column: usize,
    },
    /// Error emitted when the server version cannot be parsed when negotiating a version
    #[error("Failed to parse API version: {api_version}")]
    APIVersionParseError {
        /// The api version returned by the server.
        api_version: String,
    },
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
    /// Error emitted when a request times out.
    #[error("Timeout error")]
    RequestTimeoutError,
    /// Error emitted when serde fails to urlencod a struct of options
    #[error(transparent)]
    URLEncodedError {
        /// The original error emitted.
        #[from]
        err: serde_urlencoded::ser::Error,
    },
}
