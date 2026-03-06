//! Credentials management, for access to the Docker Hub or a custom Registry.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
#[cfg(feature = "with-env")]
use std::fs::File;
#[cfg(feature = "with-env")]
use std::io::BufReader;
#[cfg(feature = "with-env")]
use std::path::{Path, PathBuf};

#[cfg(feature = "with-env")]
use crate::errors::Error;

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

/// Default Docker configuration filename inside the config directory.
#[cfg(feature = "with-env")]
pub const DOCKER_CONFIG_FILENAME: &str = "config.json";

/// Canonical name for the Docker Hub registry.
#[cfg(feature = "with-env")]
pub const INDEX_NAME: &str = "docker.io";

#[cfg(feature = "with-env")]
/// Parsed Docker client configuration, loaded from `~/.docker/config.json`.
///
/// Loaded automatically when a [`Docker`](crate::Docker) client is created.
/// Contains inline credentials and references to external credential helpers.
/// Use [`Docker::credentials_for`](crate::Docker::credentials_for) to resolve
/// credentials for a specific registry.
#[derive(Debug, Clone, Default)]
pub struct DockerConfig {
    /// Inline credentials from the `auths` section of `config.json`, keyed
    /// by registry URL or hostname.
    pub auths: HashMap<String, DockerCredentials>,
    /// Global credential store helper name (e.g. `"osxkeychain"`).
    pub creds_store: Option<String>,
    /// Per-registry credential helpers, keyed by registry hostname.
    pub cred_helpers: HashMap<String, String>,
}

#[cfg(feature = "with-env")]
impl DockerConfig {
    /// Load the Docker configuration from the standard location.
    ///
    /// Search order:
    /// 1. `$DOCKER_CONFIG/config.json` (if `DOCKER_CONFIG` env var is set)
    /// 2. `~/.docker/config.json`
    ///
    /// Returns an empty config if no file is found or parsing fails.
    pub fn load() -> Result<Self, Error> {
        let Some(path) = find_config_path() else {
            return Ok(Self::default());
        };
        Self::load_from_file(&path)
    }

    fn load_from_file(path: &Path) -> Result<Self, Error> {
        let file = File::open(path)
            .map_err(|_| Error::DockerConfigParseError(path.display().to_string()))?;
        serde_json::from_reader::<_, RawDockerConfig>(BufReader::new(file))
            .map(Into::into)
            .map_err(|_| Error::DockerConfigParseError(path.display().to_string()))
    }

    /// Resolve credentials for the given registry, including via external
    /// credential helpers (`credsStore` / `credHelpers`).
    ///
    /// `registry` may be a hostname (`docker.io`, `gcr.io`) or a full URL
    /// (`https://index.docker.io/v1/`). The lookup order is:
    ///
    /// 1. Per-registry credential helper (`credHelpers`)
    /// 2. Inline `auth` or `identitytoken` in the `auths` section
    /// 3. Global credential store (`credsStore`)
    ///
    /// Returns `None` if no credentials are found or the credential helper
    /// fails (e.g. the helper binary is not installed).
    pub fn credentials_for_registry(&self, registry: &str) -> Option<DockerCredentials> {
        match docker_credential::get_credential(registry) {
            Ok(docker_credential::DockerCredential::UsernamePassword(username, password)) => {
                Some(DockerCredentials {
                    username: Some(username),
                    password: Some(password),
                    serveraddress: Some(registry.to_string()),
                    ..Default::default()
                })
            }
            Ok(docker_credential::DockerCredential::IdentityToken(token)) => {
                Some(DockerCredentials {
                    identitytoken: Some(token),
                    serveraddress: Some(registry.to_string()),
                    ..Default::default()
                })
            }
            Err(_) => None,
        }
    }

    /// Retrieve credentials for every registry that appears in the `auths`
    /// or `credHelpers` sections of the config file.
    ///
    /// Suitable for populating the `X-Registry-Config` header used by
    /// multi-registry operations such as
    /// [`Docker::build_image`](crate::Docker::build_image).
    ///
    /// Each registry is resolved via [`credentials_for_registry`](Self::credentials_for_registry),
    /// so credential helpers are invoked as needed. Registries whose helper
    /// fails or returns no credentials are silently omitted.
    pub fn all_credentials(&self) -> HashMap<String, DockerCredentials> {
        let mut result = HashMap::new();
        for registry in self.auths.keys().chain(self.cred_helpers.keys()) {
            if let Some(creds) = self.credentials_for_registry(registry) {
                result.insert(registry.clone(), creds);
            }
        }
        result
    }
}

#[cfg(feature = "with-env")]
fn find_config_path() -> Option<PathBuf> {
    // $DOCKER_CONFIG takes priority
    if let Ok(dir) = std::env::var("DOCKER_CONFIG") {
        let path = PathBuf::from(dir).join(DOCKER_CONFIG_FILENAME);
        if path.exists() {
            return Some(path);
        }
    }
    // Fall back to ~/.docker/config.json
    let home = docker_home_dir()?;
    let path = home.join(".docker").join(DOCKER_CONFIG_FILENAME);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

#[cfg(feature = "with-env")]
fn docker_home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg(feature = "with-env")]
/// Raw deserialization target for `~/.docker/config.json`.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RawDockerConfig {
    auths: HashMap<String, RawAuthEntry>,
    creds_store: Option<String>,
    cred_helpers: HashMap<String, String>,
}

#[cfg(feature = "with-env")]
/// Raw deserialization target for a single entry inside the `auths` map.
#[derive(Deserialize, Default)]
struct RawAuthEntry {
    /// Base64-encoded `"username:password"`.
    auth: Option<String>,
    email: Option<String>,
    identitytoken: Option<String>,
}

#[cfg(feature = "with-env")]
impl From<RawDockerConfig> for DockerConfig {
    fn from(raw: RawDockerConfig) -> Self {
        let auths = raw
            .auths
            .into_iter()
            .map(|(registry, entry)| {
                let mut creds = DockerCredentials {
                    serveraddress: Some(registry.clone()),
                    email: entry.email,
                    identitytoken: entry.identitytoken,
                    ..Default::default()
                };
                if let Some(auth_b64) = entry.auth {
                    if let Ok(decoded) = STANDARD.decode(auth_b64.trim()) {
                        if let Ok(s) = String::from_utf8(decoded) {
                            if let Some((user, pass)) = s.split_once(':') {
                                creds.username = Some(user.to_string());
                                creds.password = Some(pass.to_string());
                            }
                        }
                    }
                }
                (registry, creds)
            })
            .collect();

        Self {
            auths,
            creds_store: raw.creds_store,
            cred_helpers: raw.cred_helpers,
        }
    }
}

#[cfg(all(test, feature = "with-env"))]
mod tests {
    use crate::auth::{DockerConfig, DockerCredentials};
    use base64::Engine;

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

    #[test]
    fn test_load_from_file_parses_auths() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(
            tmp,
            r#"{{
                "auths": {{
                    "https://index.docker.io/v1/": {{
                        "auth": "{}"
                    }}
                }}
            }}"#,
            base64::engine::general_purpose::STANDARD.encode("alice:password123")
        )
        .unwrap();
        let config = DockerConfig::load_from_file(tmp.path()).unwrap();
        let creds = config.auths.get("https://index.docker.io/v1/").unwrap();
        assert_eq!(creds.username.as_deref(), Some("alice"));
        assert_eq!(creds.password.as_deref(), Some("password123"));
    }

    #[test]
    fn test_load_from_file_returns_error_on_invalid_json() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "not valid json").unwrap();
        let result = DockerConfig::load_from_file(tmp.path());
        assert!(matches!(
            result,
            Err(crate::errors::Error::DockerConfigParseError(_))
        ));
    }

    #[test]
    fn test_load_from_file_returns_error_on_missing_file() {
        let result = DockerConfig::load_from_file(std::path::Path::new(
            "/tmp/bollard_test_nonexistent_config_xyz.json",
        ));
        assert!(matches!(
            result,
            Err(crate::errors::Error::DockerConfigParseError(_))
        ));
    }

    #[test]
    fn test_load_from_file_empty_config() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "{{}}").unwrap();
        let config = DockerConfig::load_from_file(tmp.path()).unwrap();
        assert!(config.auths.is_empty());
        assert!(config.creds_store.is_none());
        assert!(config.cred_helpers.is_empty());
    }
}
