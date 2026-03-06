//! Credentials management, for access to the Docker Hub or a custom Registry.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
/// DockerCredentials credentials and server URI to push images using the [Push Image
/// API](crate::Docker::push_image()) or the [Build Image
/// API](../struct.Docker.html#method.build_image).
pub struct DockerCredentials {
    pub username: Option<String>,
    pub password: Option<String>,
    pub auth: Option<String>,
    pub email: Option<String>,
    pub serveraddress: Option<String>,
    pub identitytoken: Option<String>,
    pub registrytoken: Option<String>,
}

fn mask_secret(s: &str) -> String {
    if s.len() <= 5 {
        "*****".to_string()
    } else {
        format!("{}*****", &s[..5])
    }
}

impl fmt::Debug for DockerCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DockerCredentials")
            .field("username", &self.username)
            .field("password", &self.password.as_deref().map(mask_secret))
            .field("auth", &self.auth.as_deref().map(mask_secret))
            .field("email", &self.email)
            .field("serveraddress", &self.serveraddress)
            .field(
                "identitytoken",
                &self.identitytoken.as_deref().map(mask_secret),
            )
            .field(
                "registrytoken",
                &self.registrytoken.as_deref().map(mask_secret),
            )
            .finish()
    }
}

pub(crate) enum DockerCredentialsHeader {
    /// Credentials of a single registry sent as an X-Registry-Auth header
    Auth(Option<DockerCredentials>),
    /// Credentials of multiple registries sent as an X-Registry-Config header
    Config(Option<HashMap<String, DockerCredentials>>),
}

pub(crate) fn base64_url_encode(payload: &str) -> String {
    STANDARD.encode(payload)
}

#[cfg(test)]
mod tests {
    use super::DockerCredentials;

    #[test]
    fn test_credentials_debug_masks_password() {
        let creds = DockerCredentials {
            username: Some("alice".to_string()),
            password: Some("supersecret".to_string()),
            ..Default::default()
        };
        let debug = format!("{creds:?}");
        assert!(debug.contains("super*****"));
        assert!(!debug.contains("supersecret"));
    }

    #[test]
    fn test_credentials_debug_masks_short_password() {
        let creds = DockerCredentials {
            password: Some("abc".to_string()),
            ..Default::default()
        };
        let debug = format!("{creds:?}");
        assert!(debug.contains("\"*****\""));
        assert!(!debug.contains("abc"));
    }

    #[test]
    fn test_credentials_debug_masks_auth_and_tokens() {
        let creds = DockerCredentials {
            auth: Some("dXNlcjpwYXNz".to_string()),
            identitytoken: Some("eyJhbGciOiJSUzI1NiJ9.payload".to_string()),
            registrytoken: Some("tok123456".to_string()),
            ..Default::default()
        };
        let debug = format!("{creds:?}");
        assert!(debug.contains("dXNlc*****"));
        assert!(debug.contains("eyJhb*****"));
        assert!(debug.contains("tok12*****"));
        assert!(!debug.contains("dXNlcjpwYXNz"));
        assert!(!debug.contains("payload"));
        assert!(!debug.contains("tok123456"));
    }

    #[test]
    fn test_credentials_debug_shows_non_sensitive_fields() {
        let creds = DockerCredentials {
            username: Some("alice".to_string()),
            email: Some("alice@example.com".to_string()),
            serveraddress: Some("https://index.docker.io/v1/".to_string()),
            ..Default::default()
        };
        let debug = format!("{creds:?}");
        assert!(debug.contains("alice"));
        assert!(debug.contains("alice@example.com"));
        assert!(debug.contains("https://index.docker.io/v1/"));
    }
}
