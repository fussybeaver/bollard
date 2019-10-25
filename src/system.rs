//! System API: interface for interacting with the Docker server and/or Registry.

use arrayvec::ArrayVec;
use chrono::serde::{ts_nanoseconds, ts_seconds};
use chrono::{DateTime, Utc};
use futures::{stream, Stream};
use http::request::Builder;
use hyper::{Body, Method};

use std::collections::HashMap;
use std::hash::Hash;

use super::{Docker, DockerChain};
#[cfg(test)]
use crate::docker::API_DEFAULT_VERSION;
use crate::either::EitherStream;
use crate::errors::Error;
use crate::errors::ErrorKind::JsonSerializeError;

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

/// Parameters used in the [Events API](../struct.Docker.html#method.events)
///
/// ## Examples
///
/// ```rust
/// # extern crate chrono;
/// use bollard::system::EventsOptions;
/// use chrono::{Duration, Utc};
/// use std::collections::HashMap;
///
/// # fn main() {
/// EventsOptions::<String>{
///     since: Utc::now() - Duration::minutes(20),
///     until: Utc::now(),
///     filters: HashMap::new()
/// };
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct EventsOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Show events created since this timestamp then stream new events.
    pub since: DateTime<Utc>,
    /// Show events created until this timestamp then stop streaming.
    pub until: DateTime<Utc>,
    /// A JSON encoded value of filters (a `map[string][]string`) to process on the event list. Available filters:
    ///  - `config=<string>` config name or ID
    ///  - `container=<string>` container name or ID
    ///  - `daemon=<string>` daemon name or ID
    ///  - `event=<string>` event type
    ///  - `image=<string>` image name or ID
    ///  - `label=<string>` image or container label
    ///  - `network=<string>` network name or ID
    ///  - `node=<string>` node ID
    ///  - `plugin`= plugin name or ID
    ///  - `scope`= local or swarm
    ///  - `secret=<string>` secret name or ID
    ///  - `service=<string>` service name or ID
    ///  - `type=<string>` object to filter by, one of `container`, `image`, `volume`, `network`, `daemon`, `plugin`, `node`, `service`, `secret` or `config`
    ///  - `volume=<string>` volume name
    pub filters: HashMap<T, Vec<T>>,
}

/// Trait providing implementations for [Events Options](struct.EventsOptions.html).
#[allow(missing_docs)]
pub trait EventsQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 3]>, Error>;
}

impl<'a, T: AsRef<str> + Eq + Hash> EventsQueryParams<&'a str, String> for EventsOptions<T>
where
    T: ::serde::Serialize,
{
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 3]>, Error> {
        Ok(ArrayVec::from([
            ("since", self.since.timestamp().to_string()),
            ("until", self.until.timestamp().to_string()),
            (
                "filters",
                serde_json::to_string(&self.filters).map_err(|e| JsonSerializeError { err: e })?,
            ),
        ]))
    }
}

/// Actor returned in the [Events API](../struct.Docker.html#method.events)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct EventsActorResults {
    #[serde(rename = "ID")]
    pub id: String,
    pub attributes: HashMap<String, String>,
}

/// Result type for the [Events API](../struct.Docker.html#method.events)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct EventsResults {
    #[serde(rename = "Type")]
    pub type_: String,
    pub action: String,
    pub actor: EventsActorResults,
    #[serde(rename = "time", with = "ts_seconds")]
    pub time: DateTime<Utc>,
    #[serde(rename = "timeNano", with = "ts_nanoseconds")]
    pub time_nano: DateTime<Utc>,
    #[serde(rename = "scope")]
    pub scope: String,
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
    pub async fn version(&self) -> Result<Version, Error> {
        let req = self.build_request::<_, String, String>(
            "/version",
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
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
    pub async fn ping(&self) -> Result<String, Error> {
        let url = "/_ping";

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Events
    ///
    /// Stream real-time events from the server.
    ///
    /// # Returns
    ///
    ///  - [Events Results](container/struct.EventsResults.html), wrapped in a
    ///  Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate chrono;
    /// use bollard::system::EventsOptions;
    /// use chrono::{Duration, Utc};
    /// use std::collections::HashMap;
    ///
    /// # fn main() {
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.events(Some(EventsOptions::<String> {
    ///     since: Utc::now() - Duration::minutes(20),
    ///     until: Utc::now(),
    ///     filters: HashMap::new(),
    /// }));
    /// # }
    /// ```
    pub fn events<T, K, V>(
        &self,
        options: Option<T>,
    ) -> impl Stream<Item = EventsResults, Error = Error>
    where
        T: EventsQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = "/events";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_stream(req)
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
    pub async fn version(self) -> Result<(DockerChain, Version), Error> {
        self.inner.version().await.map(|result| (self, result))
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
    pub async fn ping(self) -> Result<(DockerChain, String), Error> {
        self.inner.ping().await.map(|result| (self, result))
    }

    /// ---
    ///
    /// # Events
    ///
    /// Stream real-time events from the server.. This is a non-blocking operation, the resulting stream will
    /// end when the container stops. Consumes the instance.
    ///
    /// # Arguments
    ///
    /// - [Events Options](container/struct.EventsOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a
    ///  [Events Results](container/struct.EventsResults.html), wrapped in a Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate chrono;
    /// use bollard::system::EventsOptions;
    /// use chrono::{Duration, Utc};
    /// use std::collections::HashMap;
    ///
    /// # fn main() {
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// let options = Some(EventsOptions::<String>{
    ///     since: Utc::now() - Duration::minutes(20),
    ///     until: Utc::now(),
    ///     filters: HashMap::new(),
    /// });
    ///
    /// docker.chain().events(options);
    /// # }
    /// ```
    pub fn events<T, K, V>(
        self,
        options: Option<T>,
    ) -> impl Future<
        Item = (
            DockerChain,
            impl Stream<Item = EventsResults, Error = Error>,
        ),
        Error = Error,
    >
    where
        T: EventsQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.inner
            .events(options)
            .into_future()
            .map(|(first, rest)| match first {
                Some(head) => (self, EitherStream::A(stream::once(Ok(head)).chain(rest))),
                None => (self, EitherStream::B(stream::empty())),
            })
            .map_err(|(err, _)| err)
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
