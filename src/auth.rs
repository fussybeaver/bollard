//! Credentials management, for access to the Docker Hub or a custom Registry.

use base64::{engine::general_purpose::STANDARD, Engine};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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

pub(crate) fn base64_url_encode(payload: &str) -> String {
    STANDARD.encode(payload)
}
