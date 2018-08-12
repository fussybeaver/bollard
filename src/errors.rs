#[derive(Fail, Debug)]
#[fail(display = "could not fetch information about container '{}'", id)]
pub struct ContainerInfoError {
    pub id: String,
}

#[derive(Fail, Debug)]
#[fail(display = "could not connected to Docker at '{}'", host)]
pub struct CouldNotConnectError {
    pub host: String,
}

#[derive(Fail, Debug)]
#[fail(display = "could not find DOCKER_CERT_PATH")]
pub struct NoCertPathError {}

#[derive(Fail, Debug)]
#[fail(display = "could not parse JSON for {} from Docker", wanted)]
pub struct ParseError {
    pub wanted: String,
    pub input: String,
}

#[derive(Fail, Debug)]
#[fail(display = "Docker SSL support was disabled at compile time")]
pub struct SslDisabled {}

#[derive(Fail, Debug)]
#[fail(display = "could not connect to Docker at '{}' using SSL", host)]
pub struct SslError {
    pub host: String,
}

#[derive(Fail, Debug)]
#[fail(display = "unsupported Docker URL scheme '{}'", scheme)]
pub struct UnsupportedScheme {
    pub scheme: String,
}

#[derive(Fail, Debug)]
#[fail(display = "container not found with id {}", id)]
pub struct ContainerNotFoundError {
    pub id: String,
}

#[derive(Fail, Debug)]
#[fail(display = "Docker responded with status code {}", status_code)]
pub struct DockerServerError {
    pub status_code: u16,
}

#[derive(Fail, Debug)]
#[fail(display = "API queried with a bad parameter")]
pub struct BadParameterError { }

#[derive(Fail, Debug)]
#[fail(display = "API responded with a read error")]
pub struct ReadError { }
