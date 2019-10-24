//! System API: interface for interacting with the Docker server and/or Registry.
use arrayvec::ArrayVec;
use http::request::Builder;
use hyper::rt::Future;
use hyper::{Body, Method};

use super::{Docker, DockerChain};
#[cfg(test)]
use crate::docker::API_DEFAULT_VERSION;
use crate::errors::Error;

/// Result type for the [Version API](../struct.Docker.html#method.version)
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct Version {
    pub version: String,
    pub api_version: String,
    pub git_commit: String,
    pub go_version: String,
    pub os: String,
    pub arch: String,
    pub kernel_version: String,
    pub build_time: Option<String>,
    pub experimental: Option<bool>,
}

impl Docker {
    /// ---
    ///
    /// # Version
    ///
    /// Returns the version of Docker that is running and various information about the system that
    /// Docker is running on.
    ///
    /// # Returns
    ///
    ///  - [Version](version/struct.Version.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.version();
    /// ```
    pub fn version(&self) -> impl Future<Item = Version, Error = Error> {
        let req = self.build_request::<_, String, String>(
            "/version",
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
    }

    /// ---
    ///
    /// # Ping
    ///
    /// This is a dummy endpoint you can use to test if the server is accessible.
    ///
    /// # Returns
    ///
    ///  - A String, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.ping();
    /// ```
    pub fn ping(&self) -> impl Future<Item = String, Error = Error> {
        let url = "/_ping";

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
    }
}

impl DockerChain {
    /// ---
    ///
    /// # Version
    ///
    /// Returns the version of Docker that is running and various information about the system that
    /// Docker is running on. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a
    ///  [Version](version/struct.Version.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.chain().version();
    /// ```
    pub fn version(self) -> impl Future<Item = (DockerChain, Version), Error = Error> {
        self.inner.version().map(|result| (self, result))
    }

    /// ---
    ///
    /// # Ping
    ///
    /// This is a dummy endpoint you can use to test if the server is accessible. Consumes the
    /// client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a
    ///  String, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.chain().ping();
    /// ```
    pub fn ping(self) -> impl Future<Item = (DockerChain, String), Error = Error> {
        self.inner.ping().map(|result| (self, result))
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::hyper_mock::HostToReplyConnector;
    use tokio::runtime::Runtime;

    #[test]
    fn test_downversion() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();

        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 875\r\n\r\n{\"Platform\":{\"Name\":\"Docker Engine - Community\"},\"Components\":[{\"Name\":\"Engine\",\"Version\":\"19.03.0-rc2\",\"Details\":{\"ApiVersion\":\"1.40\",\"Arch\":\"amd64\",\"BuildTime\":\"2019-06-05T01:42:10.000000000+00:00\",\"Experimental\":\"true\",\"GitCommit\":\"f97efcc\",\"GoVersion\":\"go1.12.5\",\"KernelVersion\":\"4.9.125-linuxkit\",\"MinAPIVersion\":\"1.12\",\"Os\":\"linux\"}},{\"Name\":\"containerd\",\"Version\":\"v1.2.6\",\"Details\":{\"GitCommit\":\"894b81a4b802e4eb2a91d1ce216b8817763c29fb\"}},{\"Name\":\"runc\",\"Version\":\"1.0.0-rc8\",\"Details\":{\"GitCommit\":\"425e105d5a03fabd737a126ad93d62a9eeede87f\"}},{\"Name\":\"docker-init\",\"Version\":\"0.18.0\",\"Details\":{\"GitCommit\":\"fec3683\"}}],\"Version\":\"19.03.0-rc2\",\"ApiVersion\":\"1.24\",\"MinAPIVersion\":\"1.12\",\"GitCommit\":\"f97efcc\",\"GoVersion\":\"go1.12.5\",\"Os\":\"linux\",\"Arch\":\"amd64\",\"KernelVersion\":\"4.9.125-linuxkit\",\"Experimental\":true,\"BuildTime\":\"2019-06-05T01:42:10.000000000+00:00\"}\r\n\r\n".to_string()
        );

        let docker =
            Docker::connect_with_host_to_reply(connector, "_".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let results = docker
            .negotiate_version()
            .and_then(|docker| docker.version());

        let future = results.map(|result| assert_eq!(result.api_version, "1.24".to_string()));

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }
}
