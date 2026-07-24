//! System API: interface for interacting with the Docker server and/or Registry.

use bytes::Bytes;
use futures_core::Stream;
use http::request::Builder;
use http_body_util::Full;
use hyper::Method;

use super::Docker;
use crate::docker::BodyType;
use crate::errors::Error;
use crate::models::*;

/// Result of the [`Docker::ping_info`] method — the daemon-advertised `/_ping`
/// response headers that the body-only [`Docker::ping`] discards. Mirrors the
/// fields of moby's own hand-written `types.Ping`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PingInfo {
    /// The daemon's (maximum) API version — the `Api-Version` response header.
    pub api_version: Option<String>,
    /// The builder the daemon advertises as its default: `"1"` (classic) or
    /// `"2"` (BuildKit) — the `Builder-Version` response header.
    pub builder_version: Option<String>,
    /// The daemon's operating system — the `OSType` response header.
    pub os_type: Option<String>,
    /// Whether the daemon is running in experimental mode — the
    /// `Docker-Experimental` response header (`"true"`).
    pub experimental: bool,
    /// The daemon's swarm membership, when it advertises one — parsed from the
    /// `Swarm` response header. `None` when the header is absent or empty.
    pub swarm_status: Option<SwarmStatus>,
}

/// The daemon's swarm membership, as advertised in the `/_ping` `Swarm`
/// response header (`<state>/<role>`). Mirrors moby's own `swarm.Status`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SwarmStatus {
    /// This node's swarm membership state — the state component of the `Swarm`
    /// header (e.g. `active`, `inactive`, `pending`).
    pub node_state: LocalNodeState,
    /// Whether this node can serve swarm control-plane requests — true when the
    /// header's role component is `manager`.
    pub control_available: bool,
}

impl PingInfo {
    /// Build a [`PingInfo`] from a `/_ping` response's headers, matching moby's
    /// own `newPingResult` header parsing.
    fn from_headers(headers: &http::HeaderMap) -> Self {
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(String::from)
        };

        let swarm_status = header("swarm").filter(|s| !s.is_empty()).map(|s| {
            let (state, role) = s.split_once('/').unwrap_or((&s, ""));
            SwarmStatus {
                node_state: state.parse().unwrap_or_default(),
                control_available: role == "manager",
            }
        });

        PingInfo {
            api_version: header("api-version"),
            builder_version: header("builder-version"),
            os_type: header("ostype"),
            experimental: header("docker-experimental").as_deref() == Some("true"),
            swarm_status,
        }
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
    ///  - [SystemVersion](crate::models::SystemVersion), wrapped in a Future.
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
    /// # Ping Info
    ///
    /// [`Docker::ping`], but returning the response *headers* the plain body-only
    /// variant discards — most notably `Builder-Version`, the builder the daemon
    /// itself advertises as its default (`"1"` classic, `"2"` BuildKit), which is
    /// what the docker CLI uses to decide its own default builder.
    ///
    /// # Returns
    ///
    ///  - A [PingInfo], wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.ping_info();
    /// ```
    pub async fn ping_info(&self) -> Result<PingInfo, Error> {
        let url = "/_ping";

        let ping = |method: Method| {
            self.process_request(self.build_request(
                url,
                Builder::new().method(method),
                None::<String>,
                Ok(BodyType::Left(Full::new(Bytes::new()))),
            ))
        };

        // Match the docker CLI: try a cheap HEAD first (no body to drain), and
        // fall back to GET if the daemon rejects it — older daemons answer HEAD
        // on `/_ping` with a non-OK status, which `process_request` surfaces as
        // an error.
        let response = match ping(Method::HEAD).await {
            Ok(response) => response,
            Err(_) => ping(Method::GET).await?,
        };

        let info = PingInfo::from_headers(response.headers());

        // Drain the body (empty for HEAD, a short `OK` for GET) so the
        // connection can return to hyper's pool for keep-alive reuse.
        Docker::discard_body(response).await?;

        Ok(info)
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
    /// use bollard::query_parameters::EventsOptionsBuilder;
    /// use std::collections::HashMap;
    ///
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("type", vec!["container"]);
    ///
    /// let options = EventsOptionsBuilder::default()
    ///     .since("1h")
    ///     .filters(&filters)
    ///     .build();
    ///
    /// docker.events(Some(options));
    /// ```
    pub fn events(
        &self,
        options: Option<crate::query_parameters::EventsOptions>,
    ) -> impl Stream<Item = Result<EventMessage, Error>> {
        let url = "/events";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            options,
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

#[cfg(test)]
mod tests {
    use super::{PingInfo, SwarmStatus};
    use crate::models::LocalNodeState;
    use http::HeaderMap;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        headers
    }

    #[test]
    fn parses_the_core_ping_headers() {
        let info = PingInfo::from_headers(&headers(&[
            ("Api-Version", "1.45"),
            ("Builder-Version", "2"),
            ("OSType", "linux"),
            ("Docker-Experimental", "true"),
        ]));

        assert_eq!(
            info,
            PingInfo {
                api_version: Some(String::from("1.45")),
                builder_version: Some(String::from("2")),
                os_type: Some(String::from("linux")),
                experimental: true,
                swarm_status: None,
            }
        );
    }

    #[test]
    fn absent_or_non_true_experimental_header_is_false() {
        assert!(!PingInfo::from_headers(&headers(&[])).experimental);
        assert!(
            !PingInfo::from_headers(&headers(&[("Docker-Experimental", "false")])).experimental
        );
    }

    #[test]
    fn parses_a_manager_swarm_header() {
        let info = PingInfo::from_headers(&headers(&[("Swarm", "active/manager")]));
        assert_eq!(
            info.swarm_status,
            Some(SwarmStatus {
                node_state: LocalNodeState::ACTIVE,
                control_available: true,
            })
        );
    }

    #[test]
    fn a_worker_or_stateless_swarm_header_is_not_control_available() {
        let worker = PingInfo::from_headers(&headers(&[("Swarm", "active/worker")]));
        assert_eq!(
            worker.swarm_status,
            Some(SwarmStatus {
                node_state: LocalNodeState::ACTIVE,
                control_available: false,
            })
        );

        // No role component at all — Docker sends a bare state when not in a swarm.
        let bare = PingInfo::from_headers(&headers(&[("Swarm", "inactive")]));
        assert_eq!(
            bare.swarm_status,
            Some(SwarmStatus {
                node_state: LocalNodeState::INACTIVE,
                control_available: false,
            })
        );
    }

    #[test]
    fn an_empty_or_absent_swarm_header_yields_no_status() {
        assert_eq!(PingInfo::from_headers(&headers(&[])).swarm_status, None);
        assert_eq!(
            PingInfo::from_headers(&headers(&[("Swarm", "")])).swarm_status,
            None
        );
    }
}
