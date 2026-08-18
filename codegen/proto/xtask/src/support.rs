use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;

use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug)]
struct XtaskError(String);

impl Display for XtaskError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for XtaskError {}

pub fn xtask_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(XtaskError(message.into()))
}

pub fn sha256(contents: &[u8]) -> String {
    hex::encode(Sha256::digest(contents))
}

pub fn validate_commit(field: &str, value: &str) -> Result<()> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(xtask_error(format!(
            "{field} must be a 40-character hexadecimal Git revision"
        )));
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(xtask_error(format!("{field} must use lowercase hexadecimal")));
    }
    Ok(())
}

pub fn validate_path(field: &str, value: &str) -> Result<()> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || value.contains('\\')
        || value
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(xtask_error(format!(
            "{field} must be a normalized relative path: {value:?}"
        )));
    }
    Ok(())
}
