//! Swarm API: Docker swarm is a container orchestration tool, meaning that it allows the user to manage multiple containers deployed across multiple host machines.
use crate::docker::BodyType;

use hyper::Method;
use serde::{Deserialize, Serialize};

use bytes::Bytes;
use http::request::Builder;
use http_body_util::Full;

use std::cmp::Eq;
use std::hash::Hash;

use super::Docker;
use crate::errors::Error;

use crate::models::*;

/// Swam configuration used in the [Init Swarm API](Docker::init_swarm())
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InitSwarmOptions<T>
where
    T: Into<String> + Eq + Hash,
{
    /// Listen address (format: <ip|interface>[:port])
    pub listen_addr: T,
    /// Externally reachable address advertised to other nodes.
    pub advertise_addr: T,
}

/// Swam configuration used in the [Join Swarm API](Docker::join_swarm())
#[derive(Debug, Clone, Default, Serialize)]
pub struct JoinSwarmOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Externally reachable address advertised to other nodes.
    pub advertise_addr: T,
    /// Secret token for joining this swarm
    pub join_token: T,
}

/// Swam configuration used in the [Leave Swarm API](Docker::leave_swarm())
#[derive(Debug, Copy, Clone, Default, Serialize)]
pub struct LeaveSwarmOptions {
    /// Force to leave to swarm.
    pub force: bool,
}

impl Docker {
    /// ---
    ///
    /// # Init Swarm
    ///
    /// Initialize a new swarm.
    ///
    /// # Arguments
    ///
    ///  - [Init Swarm Options](InitSwarmOptions) struct.
    ///
    /// # Returns
    ///
    ///  - A String wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::network::InitSwarmOptions;
    ///
    /// use std::default::Default;
    ///
    /// let config = InitSwarmOptions {
    ///     advertiseAddr: "127.0.0.1",
    /// };
    ///
    /// docker.init_swarm(config);
    /// ```
    pub async fn init_swarm<T>(&self, config: InitSwarmOptions<T>) -> Result<String, Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/swarm/init";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Inspect Swarm
    ///
    /// Inspect swarm.
    ///
    /// # Arguments
    ///
    /// # Returns
    ///
    ///  - [Swarm](swarm) struct, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::network::InitSwarmOptions;
    /// docker.inspect_swarm();
    /// ```
    pub async fn inspect_swarm(&self) -> Result<Swarm, Error> {
        let url = "/swarm";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Join a Swarm
    ///
    /// # Arguments
    ///
    ///  - [Join Swarm Options](JoinSwarmOptions) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// let config = JoinSwarmOptions {
    ///     advertiseAddr: "127.0.0.1",
    ///     joinToken: "token",
    /// };
    /// docker.join_swarm(config);
    /// ```
    pub async fn join_swarm<T>(&self, config: JoinSwarmOptions<T>) -> Result<(), Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/swarm/join";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Leave a Swarm
    ///
    /// # Arguments
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.leave_swarm();
    /// ```
    pub async fn leave_swarm(&self, options: Option<LeaveSwarmOptions>) -> Result<(), Error> {
        let url = "/swarm/leave";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
    }
}
