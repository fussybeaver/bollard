//! Swarm API: Docker swarm is a container orchestration tool, meaning that it allows the user to manage multiple containers deployed across multiple host machines.
#![allow(deprecated)]
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
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::models::SwarmInitRequest"
)]
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

impl<T> From<InitSwarmOptions<T>> for SwarmInitRequest
where
    T: Into<String> + Eq + Hash,
{
    fn from(opts: InitSwarmOptions<T>) -> Self {
        SwarmInitRequest {
            listen_addr: Some(opts.listen_addr.into()),
            advertise_addr: Some(opts.advertise_addr.into()),
            ..Default::default()
        }
    }
}

/// Swam configuration used in the [Join Swarm API](Docker::join_swarm())
#[derive(Debug, Clone, Default, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::models::SwarmJoinRequest"
)]
pub struct JoinSwarmOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Externally reachable address advertised to other nodes.
    pub advertise_addr: T,
    /// Secret token for joining this swarm
    pub join_token: T,
}

impl<T> From<JoinSwarmOptions<T>> for SwarmJoinRequest
where
    T: Into<String> + Serialize,
{
    fn from(opts: JoinSwarmOptions<T>) -> Self {
        SwarmJoinRequest {
            advertise_addr: Some(opts.advertise_addr.into()),
            join_token: Some(opts.join_token.into()),
            ..Default::default()
        }
    }
}

/// Swam configuration used in the [Leave Swarm API](Docker::leave_swarm())
#[derive(Debug, Copy, Clone, Default, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::LeaveSwarmOptions and associated LeaveSwarmOptionsBuilder"
)]
pub struct LeaveSwarmOptions {
    /// Force to leave to swarm.
    pub force: bool,
}

impl From<LeaveSwarmOptions> for crate::query_parameters::LeaveSwarmOptions {
    fn from(opts: LeaveSwarmOptions) -> Self {
        crate::query_parameters::LeaveSwarmOptionsBuilder::default()
            .force(opts.force)
            .build()
    }
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
    ///    Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// # use bollard::swarm::InitSwarmOptions;
    ///
    /// use std::default::Default;
    ///
    /// let config = InitSwarmOptions {
    ///     advertise_addr: "127.0.0.1",
    ///     listen_addr: "0.0.0.0:2377"
    /// };
    ///
    /// docker.init_swarm(config);
    /// ```
    pub async fn init_swarm(&self, config: impl Into<SwarmInitRequest>) -> Result<String, Error> {
        let url = "/swarm/init";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            None::<String>,
            Docker::serialize_payload(Some(config.into())),
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
    /// docker.inspect_swarm();
    /// ```
    pub async fn inspect_swarm(&self) -> Result<Swarm, Error> {
        let url = "/swarm";

        let req = self.build_request(
            url,
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
    /// # use bollard::swarm::JoinSwarmOptions;
    ///
    /// let config = JoinSwarmOptions {
    ///     advertise_addr: "127.0.0.1",
    ///     join_token: "token",
    /// };
    /// docker.join_swarm(config);
    /// ```
    pub async fn join_swarm(&self, config: impl Into<SwarmJoinRequest>) -> Result<(), Error> {
        let url = "/swarm/join";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            None::<String>,
            Docker::serialize_payload(Some(config.into())),
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
    /// # use bollard::query_parameters::LeaveSwarmOptions;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.leave_swarm(None::<LeaveSwarmOptions>);
    /// ```
    pub async fn leave_swarm(
        &self,
        options: Option<impl Into<crate::query_parameters::LeaveSwarmOptions>>,
    ) -> Result<(), Error> {
        let url = "/swarm/leave";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Update a Swarm
    ///
    /// Update a swarm's configuration.
    ///
    /// # Arguments
    ///
    ///  - [SwarmSpec](SwarmSpec) struct.
    ///  - [UpdateSwarmOptions](crate::query_parameters::UpdateSwarmOptions) struct.
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
    /// use bollard::query_parameters::UpdateSwarmOptionsBuilder;
    ///
    /// let result = async move {
    ///     let swarm = docker.inspect_swarm().await?;
    ///     let version = swarm.version.unwrap().index.unwrap();
    ///     let spec = swarm.spec.unwrap();
    ///
    ///     let options = UpdateSwarmOptionsBuilder::default()
    ///         .version(version as i64)
    ///         .build();
    ///
    ///     docker.update_swarm(spec, options).await
    /// };
    /// ```
    pub async fn update_swarm(
        &self,
        swarm_spec: SwarmSpec,
        options: crate::query_parameters::UpdateSwarmOptions,
    ) -> Result<(), Error> {
        let url = "/swarm/update";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            Some(options),
            Docker::serialize_payload(Some(swarm_spec)),
        );

        self.process_into_unit(req).await
    }
}
