//! Errors for this module.
use std::cmp;
use std::fmt::{Display, Formatter, Result};

use failure::Context;

#[derive(Debug)]
/// Generic Error type over all errors in the bollard library
pub struct Error {
    inner: Context<ErrorKind>,
}

/// The type of error embedded in an Error.
#[derive(Debug, Fail)]
pub enum ErrorKind {
    #[fail(display = "could not find DOCKER_CERT_PATH")]
    /// Error emitted during client instantiation when the `DOCKER_CERT_PATH` environment variable
    /// is invalid.
    NoCertPathError,
    #[fail(display = "API responded with a 404 not found: {}", message)]
    /// Error emitted by the docker server, when it responds with a 404.
    DockerResponseNotFoundError {
        /// Message returned by the docker server.
        message: String,
    },
    #[fail(
        display = "Docker responded with status code {}: {}",
        status_code, message
    )]
    /// Generic error emitted by the docker server.
    DockerResponseServerError {
        /// Status code returned by the docker server.
        status_code: u16,
        /// Message returned by the docker server.
        message: String,
    },
    #[fail(display = "API queried with a bad parameter: {}", message)]
    /// Error emitted by the docker server, when it responds with a 400.
    DockerResponseBadParameterError {
        /// Message returned by the docker server.
        message: String,
    },
    #[fail(display = "API responded with a 409 conflict: {}", message)]
    /// Error emitted by the docker server, when it responds with a 409.
    DockerResponseConflictError {
        /// Message returned by the docker server.
        message: String,
    },
    #[fail(
        display = "API responded with a 304, resource was not modified: {}",
        message
    )]
    /// Error emitted by the docker server, when it responds with a 304.
    DockerResponseNotModifiedError {
        /// Message returned by the docker server.
        message: String,
    },
    #[fail(display = "Failed to deserialize JSON: {}", message)]
    /// Error facilitating debugging failed JSON parsing.
    JsonDataError {
        /// Short section of the json close to the error.
        message: String,
        /// Entire JSON payload.
        contents: String,
        /// Character sequence at error location.
        column: usize,
    },
    #[fail(display = "Failed to parse API version: {}", api_version)]
    /// Error emitted when the server version cannot be parsed when negotiating a version
    APIVersionParseError {
        /// The api version returned by the server.
        api_version: String,
    },
    #[fail(display = "Failed to serialize JSON: {:?}", err)]
    /// Error emitted when JSON fails to serialize.
    JsonSerializeError {
        /// The original error emitted by serde.
        err: serde_json::Error,
    },
    #[fail(display = "Failed to deserialize JSON: {}: {:?}", content, err)]
    /// Error emitted when JSON fails to deserialize.
    JsonDeserializeError {
        /// The original string that was being deserialized.
        content: String,
        /// The original error emitted by serde.
        err: serde_json::Error,
    },
    #[fail(display = "UTF8 error: {}: {:?}", content, err)]
    /// Error emitted when log output generates an I/O error.
    StrParseError {
        /// the bytes that failed
        content: String,
        /// The original error emitted.
        err: std::str::Utf8Error,
    },
    #[fail(display = "I/O error: {:?}", err)]
    /// Error emitted from an I/O error.
    IOError {
        /// The original error emitted.
        err: std::io::Error,
    },
    #[fail(display = "Format error: {}: {:?}", content, err)]
    /// Error emitted from a formatting error.
    StrFmtError {
        /// The original string that failed to format.
        content: String,
        /// The original error emitted.
        err: std::fmt::Error,
    },
    #[fail(display = "HTTP error: {}: {:?}", builder, err)]
    /// Error emitted from an HTTP error.
    HttpClientError {
        /// The client builder, formatted as a debug string.
        builder: String,
        /// The original error emitted.
        err: http::Error,
    },
    #[fail(display = "Hyper error: {:?}", err)]
    /// Error emitted from an HTTP error.
    HyperResponseError {
        /// The original error emitted.
        err: hyper::Error,
    },
    /// Error emitted when a request times out.
    #[fail(display = "Timeout error")]
    RequestTimeoutError,
    /// Error emitted when an SSL context fails to configure.
    #[cfg(feature = "openssl")]
    #[fail(display = "SSL error: {:?}", err)]
    SSLError {
        /// The original error emitted.
        err: openssl::error::ErrorStack,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.inner.get_context() {
            ErrorKind::JsonDataError {
                message,
                contents,
                column,
            } => {
                let backtrack_len: usize = 24;
                let peek_len: usize = 32;
                let description = "Failed to deserialize near ...";
                let from_start_length = column.checked_sub(backtrack_len).unwrap_or(0);
                write!(
                    f,
                    "{}{}...\n{}",
                    description,
                    &contents[from_start_length..cmp::min(contents.len(), column + peek_len)],
                    message
                )
            }
            _ => Display::fmt(&self.inner, f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.inner.get_context() {
            ErrorKind::JsonSerializeError { err, .. } => Some(err),
            ErrorKind::JsonDeserializeError { err, .. } => Some(err),
            ErrorKind::StrParseError { err, .. } => Some(err),
            ErrorKind::IOError { err, .. } => Some(err),
            ErrorKind::StrFmtError { err, .. } => Some(err),
            ErrorKind::HttpClientError { err, .. } => Some(err),
            ErrorKind::HyperResponseError { err, .. } => Some(err),
            ErrorKind::RequestTimeoutError { err, .. } => Some(err),
            _ => None,
        }
    }
}

impl Error {
    /// yield the underlying error kind of this error.
    pub fn kind(&self) -> &ErrorKind {
        self.inner.get_context()
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Error {
        Error {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<ErrorKind>> for Error {
    fn from(inner: Context<ErrorKind>) -> Error {
        Error { inner: inner }
    }
}

/// Needed due to tokio's Decoder implementation
impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error {
            inner: ErrorKind::IOError { err: err }.into(),
        }
    }
}
