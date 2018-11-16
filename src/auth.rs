//! Credentials management, for access to the Docker Hub or a custom Registry.

#[derive(Debug, Clone, Default, Serialize)]
#[allow(missing_docs)]
/// DockerCredentials credentials and server URI to push images using the [Push Image
/// API](../struct.Docker.html#method.push_image).
pub struct DockerCredentials {
    pub username: Option<String>,
    pub password: Option<String>,
    pub email: Option<String>,
    pub serveraddress: Option<String>,
}
