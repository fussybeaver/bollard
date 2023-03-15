use bollard_next::system::Version;
#[cfg(unix)]
use bollard_next::ClientVersion;
use bollard_next::Docker;
use tokio::runtime::Runtime;

#[macro_use]
mod common;

#[cfg(windows)]
#[test]
fn test_version_named_pipe() {
    rt_exec!(
        Docker::connect_with_named_pipe_defaults()
            .unwrap()
            .version(),
        |version: Version| assert_eq!(version.os.unwrap(), "windows")
    )
}

#[cfg(all(unix, not(feature = "test_http")))]
#[test]
fn test_version_unix() {
    rt_exec!(
        Docker::connect_with_unix_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.os.unwrap(), "linux")
    )
}

#[cfg(feature = "test_ssl")]
#[test]
fn test_version_ssl() {
    rt_exec!(
        Docker::connect_with_ssl_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.os.unwrap(), "linux")
    )
}

#[cfg(feature = "test_http")]
#[test]
fn test_version_http() {
    #[cfg(unix)]
    rt_exec!(
        Docker::connect_with_http_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.os.unwrap(), "linux")
    );
    #[cfg(windows)]
    rt_exec!(
        Docker::connect_with_http_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.os.unwrap(), "windows")
    )
}

#[cfg(unix)]
#[test]
fn test_downversioning() {
    let rt = Runtime::new().unwrap();

    let docker = Docker::connect_with_unix(
        "unix:///var/run/docker.sock",
        120,
        &ClientVersion {
            major_version: 1,
            minor_version: 24,
        },
    )
    .unwrap();

    let fut = async move {
        let docker = &docker.negotiate_version().await.unwrap();

        let _ = &docker.version().await.unwrap();

        assert_eq!(docker.client_version().to_string(), "1.24".to_string());
    };
    rt.block_on(fut);
}
