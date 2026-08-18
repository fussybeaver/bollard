use std::path::Path;

use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};

pub fn sha256(contents: &[u8]) -> String {
    hex::encode(Sha256::digest(contents))
}

pub fn validate_commit(field: &str, value: &str) -> Result<()> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(anyhow!(
            "{field} must be a 40-character hexadecimal Git revision"
        ));
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(anyhow!("{field} must use lowercase hexadecimal"));
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
        return Err(anyhow!(
            "{field} must be a normalized relative path: {value:?}"
        ));
    }
    Ok(())
}
