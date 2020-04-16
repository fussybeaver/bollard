//! System API: interface for interacting with the Docker server and/or Registry.

use arrayvec::ArrayVec;
use chrono::serde::{ts_nanoseconds, ts_seconds};
use chrono::{DateTime, Utc};
use futures_core::Stream;
use http::request::Builder;
use hyper::{Body, Method};

use std::collections::HashMap;
use std::hash::Hash;

use super::Docker;
use crate::container::APIContainers;
use crate::errors::Error;
use crate::errors::ErrorKind::JsonSerializeError;
use crate::image::APIImages;

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
            ("since", format!("{}.{}", self.since.timestamp(), self.since.timestamp_subsec_nanos())),
            ("until", format!("{}.{}", self.until.timestamp(), self.until.timestamp_subsec_nanos())),
            (
                "filters",
                serde_json::to_string(&self.filters).map_err(|e| JsonSerializeError { err: e })?,
            ),
        ]))
    }
}

/// Actor returned in the [Events API](../struct.Docker.html#method.events)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct EventsActorResults {
    #[serde(rename = "ID")]
    pub id: String,
    pub attributes: HashMap<String, String>,
}

/// Result type for the [Events API](../struct.Docker.html#method.events)
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Volumes returned in the [Df API](../struct.Docker.html#method.df)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct DfVolumesUsageDataResults {
    pub size: u64,
    pub ref_count: usize,
}

/// Volumes returned in the [Df API](../struct.Docker.html#method.df)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct DfVolumesResults {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub labels: Option<HashMap<String, String>>,
    pub scope: String,
    pub options: Option<HashMap<String, String>>,
    pub usage_data: DfVolumesUsageDataResults,
}

/// Result type for the [Df API](../struct.Docker.html#method.df)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct DfResults {
    pub layers_size: u64,
    pub images: Vec<APIImages>,
    pub containers: Vec<APIContainers>,
    pub volumes: Vec<DfVolumesResults>,
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
    ///  - [Version](system/struct.Version.html), wrapped in a Future.
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
    ///  - [Events Results](system/struct.EventsResults.html), wrapped in a
    ///  Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bollard::system::EventsOptions;
    /// use chrono::{Duration, Utc};
    /// use std::collections::HashMap;
    ///
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.events(Some(EventsOptions::<String> {
    ///     since: Utc::now() - Duration::minutes(20),
    ///     until: Utc::now(),
    ///     filters: HashMap::new(),
    /// }));
    /// ```
    pub fn events<T, K, V>(
        &self,
        options: Option<T>,
    ) -> impl Stream<Item = Result<EventsResults, Error>>
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

    /// ---
    ///
    /// # Get data usage information
    ///
    /// Show docker disk usage
    ///
    /// # Returns
    ///
    ///  - [Df Results](system/struct.DfResults.html), wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.df();
    /// ```
    pub async fn df(&self) -> Result<DfResults, Error> {
        let url = "/system/df";

        let req = self.build_request::<_, String, String>(
            url,
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }
}
