//! System API: interface for interacting with the Docker server and/or Registry.
#![allow(deprecated)]

use bytes::Bytes;
use futures_core::Stream;
use http::request::Builder;
use http_body_util::Full;
use hyper::Method;
use serde_derive::Serialize;

use std::collections::HashMap;
use std::hash::Hash;

use super::Docker;
use crate::docker::BodyType;
use crate::errors::Error;
use crate::models::*;

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
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::EventsOptions and associated EventsOptionsBuilder"
)]
pub struct EventsOptions<T>
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    /// Show events created since this timestamp then stream new events.
    #[cfg(all(feature = "chrono", not(feature = "time")))]
    #[serde(serialize_with = "crate::docker::serialize_as_timestamp")]
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Show events created until this timestamp then stop streaming.
    #[cfg(all(feature = "chrono", not(feature = "time")))]
    #[serde(serialize_with = "crate::docker::serialize_as_timestamp")]
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    /// Show events created since this timestamp then stream new events.
    #[cfg(feature = "time")]
    #[serde(serialize_with = "crate::docker::serialize_as_timestamp")]
    pub since: Option<time::OffsetDateTime>,
    /// Show events created until this timestamp then stop streaming.
    #[cfg(feature = "time")]
    #[serde(serialize_with = "crate::docker::serialize_as_timestamp")]
    pub until: Option<time::OffsetDateTime>,
    /// Show events created since this timestamp then stream new events.
    #[cfg(not(any(feature = "time", feature = "chrono")))]
    pub since: Option<String>,
    /// Show events created until this timestamp then stop streaming.
    #[cfg(not(any(feature = "time", feature = "chrono")))]
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

impl<T> From<EventsOptions<T>> for crate::query_parameters::EventsOptions
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    fn from(opts: EventsOptions<T>) -> Self {
        let mut builder = crate::query_parameters::EventsOptionsBuilder::default().filters(
            &opts
                .filters
                .into_iter()
                .map(|(k, v)| (k.into(), v.into_iter().map(T::into).collect()))
                .collect(),
        );

        if let Some(since) = opts.since {
            builder = builder.since(
                #[cfg(all(feature = "chrono", not(feature = "time")))]
                &format!("{}.{}", since.timestamp(), since.timestamp_subsec_nanos()),
                #[cfg(feature = "time")]
                &format!(
                    "{}.{}",
                    since.unix_timestamp(),
                    since.unix_timestamp_nanos()
                ),
                #[cfg(not(any(feature = "time", feature = "chrono")))]
                &since,
            );
        }

        if let Some(until) = opts.until {
            builder = builder.until(
                #[cfg(all(feature = "chrono", not(feature = "time")))]
                &format!("{}.{}", until.timestamp(), until.timestamp_subsec_nanos()),
                #[cfg(feature = "time")]
                &format!(
                    "{}.{}",
                    until.unix_timestamp(),
                    until.unix_timestamp_nanos()
                ),
                #[cfg(not(any(feature = "time", feature = "chrono")))]
                &until,
            );
        }

        builder.build()
    }
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
    ///  - [Version](SystemVersion), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.version();
    /// ```
    pub async fn version(&self) -> Result<SystemVersion, Error> {
        let req = self.build_request(
            "/version",
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
    ///    wrapped in a Stream.
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
    pub fn events(
        &self,
        options: Option<impl Into<crate::query_parameters::EventsOptions>>,
    ) -> impl Stream<Item = Result<EventMessage, Error>> {
        let url = "/events";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
    ///    Response](SystemDataUsageResponse), wrapped in a
    ///    Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # use bollard::query_parameters::DataUsageOptions;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.df(None::<DataUsageOptions>);
    /// ```
    pub async fn df(
        &self,
        options: Option<crate::query_parameters::DataUsageOptions>,
    ) -> Result<SystemDataUsageResponse, Error> {
        let url = "/system/df";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }
}
