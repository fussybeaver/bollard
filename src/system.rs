//! System API: interface for interacting with the Docker server and/or Registry.

use futures_core::Stream;
use http::request::Builder;
use hyper::{Body, Method};
use serde::ser::Serialize;
use serde_json::value::Value;

use std::collections::HashMap;
use std::hash::Hash;

use super::Docker;
use crate::errors::Error;
use crate::models::*;

/// Response of Engine API: GET \"/version\"
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub struct Version {
    #[serde(rename = "Platform")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<SystemVersionPlatform>,

    /// Information about system components
    #[serde(rename = "Components")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<VersionComponents>>,

    /// The version of the daemon
    #[serde(rename = "Version")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// The default (and highest) API version that is supported by the daemon
    #[serde(rename = "ApiVersion")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,

    /// The minimum API version that is supported by the daemon
    #[serde(rename = "MinAPIVersion")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_api_version: Option<String>,

    /// The Git commit of the source code that was used to build the daemon
    #[serde(rename = "GitCommit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,

    /// The version Go used to compile the daemon, and the version of the Go runtime in use.
    #[serde(rename = "GoVersion")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go_version: Option<String>,

    /// The operating system that the daemon is running on (\"linux\" or \"windows\")
    #[serde(rename = "Os")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,

    /// The architecture that the daemon is running on
    #[serde(rename = "Arch")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,

    /// The kernel version (`uname -r`) that the daemon is running on.  This field is omitted when empty.
    #[serde(rename = "KernelVersion")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,

    /// Indicates if the daemon is started with experimental features enabled.  This field is omitted when empty / false.
    #[serde(rename = "Experimental")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg(windows)]
    pub experimental: Option<bool>,
    #[cfg(not(windows))]
    pub experimental: Option<String>,

    /// The date and time that the daemon was compiled.
    #[serde(rename = "BuildTime")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub struct VersionComponents {
    /// Name of the component
    #[serde(rename = "Name")]
    pub name: String,

    /// Version of the component
    #[serde(rename = "Version")]
    pub version: String,

    /// Key/value pairs of strings with additional information about the component. These values are intended for informational purposes only, and their content is not defined, and not part of the API specification.  These messages can be printed by the client as information to the user.
    #[serde(rename = "Details")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, Value>>,
}

/// Parameters used in the [Events API](Docker::events())
///
/// ## Examples
///
/// ```rust
/// use bollard::system::EventsOptions;
/// use time::{Duration, OffsetDateTime};
/// use std::collections::HashMap;
///
/// # fn main() {
/// EventsOptions::<String>{
///     since: Some(OffsetDateTime::now_utc() - Duration::minutes(20)),
///     until: Some(OffsetDateTime::now_utc()),
///     filters: HashMap::new()
/// };
/// # }
/// ```
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct EventsOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
{
    /// Show events created since this timestamp then stream new events.
    #[cfg(all(feature = "chrono", not(feature = "time")))]
    #[serde(serialize_with = "crate::docker::serialize_as_timestamp")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<Rfc3339>"))]
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Show events created until this timestamp then stop streaming.
    #[cfg(all(feature = "chrono", not(feature = "time")))]
    #[serde(serialize_with = "crate::docker::serialize_as_timestamp")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<Rfc3339>"))]
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    /// Show events created since this timestamp then stream new events.
    #[cfg(feature = "time")]
    #[serde(serialize_with = "crate::docker::serialize_as_timestamp")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<Rfc3339>"))]
    pub since: Option<time::OffsetDateTime>,
    /// Show events created until this timestamp then stop streaming.
    #[cfg(feature = "time")]
    #[serde(serialize_with = "crate::docker::serialize_as_timestamp")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<Rfc3339>"))]
    pub until: Option<time::OffsetDateTime>,
    /// Show events created since this timestamp then stream new events.
    #[cfg(not(any(feature = "time", feature = "chrono")))]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<Rfc3339>"))]
    pub since: Option<String>,
    /// Show events created until this timestamp then stop streaming.
    #[cfg(not(any(feature = "time", feature = "chrono")))]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<Rfc3339>"))]
    pub until: Option<String>,
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
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
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
    ///  - [Version](Version), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.version();
    /// ```
    pub async fn version(&self) -> Result<Version, Error> {
        let req = self.build_request(
            "/version",
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Info
    ///
    /// Returns Docker client and server information that is running.
    ///
    /// # Returns
    ///
    ///  - [Info](SystemInfo), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.info();
    /// ```
    pub async fn info(&self) -> Result<SystemInfo, Error> {
        let req = self.build_request(
            "/info",
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Ping
    ///
    /// This is a dummy endpoint you can use to test if the server is accessible.
    /// # Returns - A [String](std::string::String), wrapped in a Future. # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.ping();
    /// ```
    pub async fn ping(&self) -> Result<String, Error> {
        let url = "/_ping";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
        );

        self.process_into_string(req).await
    }

    /// ---
    ///
    /// # Events
    ///
    /// Stream real-time events from the server.
    ///
    /// # Returns
    ///
    ///  - [EventMessage](crate::models::EventMessage),
    ///  wrapped in a Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bollard::system::EventsOptions;
    /// use time::{Duration, OffsetDateTime};
    /// use std::collections::HashMap;
    ///
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.events(Some(EventsOptions::<String> {
    ///     since: Some(OffsetDateTime::now_utc() - Duration::minutes(20)),
    ///     until: Some(OffsetDateTime::now_utc()),
    ///     filters: HashMap::new(),
    /// }));
    /// ```
    pub fn events<T>(
        &self,
        options: Option<EventsOptions<T>>,
    ) -> impl Stream<Item = Result<EventMessage, Error>>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/events";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            options,
            Ok(Body::empty()),
        );

        self.process_into_stream(req)
    }

    /// ---
    ///
    /// # Get data usage information
    ///
    /// Show docker disk usage
    ///
    /// # Returns
    ///
    ///  - [System Data Usage
    ///  Response](SystemDataUsageResponse), wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.df();
    /// ```
    pub async fn df(&self) -> Result<SystemDataUsageResponse, Error> {
        let url = "/system/df";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }
}
