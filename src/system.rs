//! System API: interface for interacting with the Docker server and/or Registry.
use arrayvec::ArrayVec;
use failure::Error;
use http::request::Builder;
use hyper::client::connect::Connect;
use hyper::rt::Future;
use hyper::{Body, Method};

use super::{Docker, DockerChain};

/// Result type for the [Version API](../struct.Docker.html#method.version)
#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
#[allow(missing_docs)]
pub struct Version {
    pub Version: String,
    pub ApiVersion: String,
    pub GitCommit: String,
    pub GoVersion: String,
    pub Os: String,
    pub Arch: String,
    pub KernelVersion: String,
    pub BuildTime: Option<String>,
    pub Experimental: Option<bool>,
}

impl<C> Docker<C>
where
    C: Connect + Sync + 'static,
{
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

impl<C> DockerChain<C>
where
    C: Connect + Sync + 'static,
{
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
    pub fn version(self) -> impl Future<Item = (DockerChain<C>, Version), Error = Error> {
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
    pub fn ping(self) -> impl Future<Item = (DockerChain<C>, String), Error = Error> {
        self.inner.ping().map(|result| (self, result))
    }
}
