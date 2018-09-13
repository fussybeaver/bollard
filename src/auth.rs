#[derive(Debug, Clone, Default, Serialize)]
pub struct DockerCredentials {
    pub username: Option<String>,
    pub password: Option<String>,
    pub email: Option<String>,
    pub serveraddress: Option<String>,
}
