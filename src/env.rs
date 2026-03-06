//! Docker client configuration, loaded from `~/.docker/config.json`.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_derive::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::auth::DockerCredentials;
use crate::errors::Error;

/// Default Docker configuration filename inside the config directory.
pub const DOCKER_CONFIG_FILENAME: &str = "config.json";

/// Canonical name for the Docker Hub registry.
pub const INDEX_NAME: &str = "docker.io";

/// Parsed Docker client configuration, loaded from `~/.docker/config.json`.
///
/// Contains inline credentials and references to external credential helpers.
/// Use [`DockerConfig::credentials_for_registry`] to resolve credentials for a
/// specific registry, or attach it to a [`Docker`](crate::Docker) client via
/// [`Docker::with_config`](crate::Docker::with_config) for automatic credential
/// resolution on image operations.
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

impl DockerConfig {
    /// Load the Docker configuration from the standard location.
    ///
    /// Search order:
    /// 1. `$DOCKER_CONFIG/config.json` (if `DOCKER_CONFIG` env var is set)
    /// 2. `~/.docker/config.json`
    ///
    /// Returns an empty config if no file is found or parsing fails.
    pub async fn load() -> Result<Self, Error> {
        let Some(path) = find_config_path() else {
            return Ok(Self::default());
        };
        Self::load_from_file(&path).await
    }

    pub(crate) async fn load_from_file(path: &Path) -> Result<Self, Error> {
        let contents = tokio::fs::read(path)
            .await
            .map_err(|_| Error::DockerConfigParseError(path.display().to_string()))?;
        serde_json::from_slice::<RawDockerConfig>(&contents)
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

fn find_config_path() -> Option<PathBuf> {
    // $DOCKER_CONFIG takes priority
    if let Ok(dir) = std::env::var("DOCKER_CONFIG") {
        let path = PathBuf::from(dir).join(DOCKER_CONFIG_FILENAME);
        if path.exists() {
            return Some(path);
        }
    }
    // Fall back to ~/.docker/config.json
    let base = directories::BaseDirs::new()?;
    let path = base.home_dir().join(".docker").join(DOCKER_CONFIG_FILENAME);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Raw deserialization target for `~/.docker/config.json`.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RawDockerConfig {
    auths: HashMap<String, RawAuthEntry>,
    creds_store: Option<String>,
    cred_helpers: HashMap<String, String>,
}

/// Raw deserialization target for a single entry inside the `auths` map.
#[derive(Deserialize, Default)]
struct RawAuthEntry {
    /// Base64-encoded `"username:password"`.
    auth: Option<String>,
    email: Option<String>,
    identitytoken: Option<String>,
}

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

#[cfg(test)]
mod tests {
    use crate::env::DockerConfig;
    use base64::Engine;

    #[tokio::test]
    async fn test_load_from_file_parses_auths() {
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
        let config = DockerConfig::load_from_file(tmp.path()).await.unwrap();
        let creds = config.auths.get("https://index.docker.io/v1/").unwrap();
        assert_eq!(creds.username.as_deref(), Some("alice"));
        assert_eq!(creds.password.as_deref(), Some("password123"));
    }

    #[tokio::test]
    async fn test_load_from_file_returns_error_on_invalid_json() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "not valid json").unwrap();
        let result = DockerConfig::load_from_file(tmp.path()).await;
        assert!(matches!(
            result,
            Err(crate::errors::Error::DockerConfigParseError(_))
        ));
    }

    #[tokio::test]
    async fn test_load_from_file_returns_error_on_missing_file() {
        let result = DockerConfig::load_from_file(std::path::Path::new(
            "/tmp/bollard_test_nonexistent_config_xyz.json",
        ))
        .await;
        assert!(matches!(
            result,
            Err(crate::errors::Error::DockerConfigParseError(_))
        ));
    }

    #[tokio::test]
    async fn test_load_from_file_empty_config() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "{{}}").unwrap();
        let config = DockerConfig::load_from_file(tmp.path()).await.unwrap();
        assert!(config.auths.is_empty());
        assert!(config.creds_store.is_none());
        assert!(config.cred_helpers.is_empty());
    }
}
