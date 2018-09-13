extern crate boondock;
extern crate hyper;
extern crate tokio;

use boondock::version::Version;
use boondock::Docker;
use hyper::rt::Future;
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
        |version: Version| assert_eq!(version.Os, "windows")
    )
}

#[cfg(all(unix, not(feature = "test_http"), not(feature = "ssl")))]
#[test]
fn test_version_unix() {
    rt_exec!(
        Docker::connect_with_unix_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.Os, "linux")
    )
}

#[cfg(feature = "ssl")]
#[test]
fn test_version_ssl() {
    rt_exec!(
        Docker::connect_with_ssl_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.Os, "linux")
    )
}

#[cfg(feature = "test-http")]
#[test]
fn test_version_http() {
    #[cfg(unix)]
    rt_exec!(
        Docker::connect_with_http_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.Os, "linux")
    );
    #[cfg(windows)]
    rt_exec!(
        Docker::connect_with_http_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.Os, "windows")
    )
}
