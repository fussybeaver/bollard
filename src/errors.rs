use std::cmp;
use std::fmt::{Display, Formatter, Result};

#[derive(Fail, Debug)]
#[fail(display = "could not find DOCKER_CERT_PATH")]
pub struct NoCertPathError {}

#[derive(Fail, Debug)]
#[fail(display = "API responded with a 404 not found: {}", message)]
pub struct DockerResponseNotFoundError {
    pub message: String,
}

#[derive(Fail, Debug)]
#[fail(display = "Docker responded with status code {}: {}", status_code, message)]
pub struct DockerResponseServerError {
    pub status_code: u16,
    pub message: String,
}

#[derive(Fail, Debug)]
#[fail(display = "API queried with a bad parameter: {}", message)]
pub struct DockerResponseBadParameterError {
    pub message: String,
}

#[derive(Fail, Debug)]
#[fail(display = "API responded with a 409 conflict: {}", message)]
pub struct DockerResponseConflictError {
    pub message: String,
}

#[derive(Fail, Debug)]
#[fail(display = "API responded with a 304, resource was not modified: {}", message)]
pub struct DockerResponseNotModifiedError {
    pub message: String,
}

#[derive(Fail, Debug)]
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
        let spaces = ::std::iter::repeat(" ")
            .take(description.len() + cmp::min(backtrack_len, self.column))
            .collect::<String>();
        write!(
            f,
            "{}{}...\n{}^---- {}",
            description,
            &self.contents
                [from_start_length..cmp::min(self.contents.len(), self.column + peek_len)],
            spaces,
            self.message
        )
    }
}
