//! Errors for this module.
use std::cmp;
use std::fmt::{Display, Formatter, Result};

/// Error emitted during client instantiation when the `DOCKER_CERT_PATH` environment variable is
/// invalid.
#[derive(Fail, Copy, Clone, Debug)]
#[fail(display = "could not find DOCKER_CERT_PATH")]
#[allow(missing_docs)]
pub struct NoCertPathError {}

/// Error emitted by the docker server, when it responds with a 404.
#[derive(Fail, Debug)]
#[fail(display = "API responded with a 404 not found: {}", message)]
#[allow(missing_docs)]
pub struct DockerResponseNotFoundError {
    pub message: String,
}

/// Generic error emitted by the docker server.
#[derive(Fail, Debug)]
#[fail(
    display = "Docker responded with status code {}: {}",
    status_code, message
)]
#[allow(missing_docs)]
pub struct DockerResponseServerError {
    pub status_code: u16,
    pub message: String,
}

/// Error emitted by the docker server, when it responds with a 400.
#[derive(Fail, Debug)]
#[fail(display = "API queried with a bad parameter: {}", message)]
#[allow(missing_docs)]
pub struct DockerResponseBadParameterError {
    pub message: String,
}

/// Error emitted by the docker server, when it responds with a 409.
#[derive(Fail, Debug)]
#[fail(display = "API responded with a 409 conflict: {}", message)]
#[allow(missing_docs)]
pub struct DockerResponseConflictError {
    pub message: String,
}

/// Error emitted by the docker server, when it responds with a 304.
#[derive(Fail, Debug)]
#[fail(
    display = "API responded with a 304, resource was not modified: {}",
    message
)]
#[allow(missing_docs)]
pub struct DockerResponseNotModifiedError {
    pub message: String,
}

/// Error facilitating debugging failed JSON parsing.
#[derive(Fail, Debug)]
#[allow(missing_docs)]
pub struct JsonDataError {
    pub message: String,
    pub contents: String,
    pub column: usize,
}

impl Display for JsonDataError {
    fn fmt(&self, f: &mut Formatter) -> Result {
        let backtrack_len: usize = 24;
        let peek_len: usize = 32;
        let description = "Failed to deserialize near ...";
        let from_start_length = self.column.checked_sub(backtrack_len).unwrap_or(0);
        write!(
            f,
            "{}{}...\n{}",
            description,
            &self.contents
                [from_start_length..cmp::min(self.contents.len(), self.column + peek_len)],
            self.message
        )
    }
}

/// Error emitted when the server version cannot be parsed when negotiating a version
#[derive(Fail, Debug)]
#[fail(display = "Failed to parse API version: {}", api_version)]
#[allow(missing_docs)]
pub struct APIVersionParseError {
    pub api_version: String,
}
