//! Network API: Networks are user-defined networks that containers can be attached to.
#![allow(deprecated)]

use bytes::Bytes;
use http::request::Builder;
use http_body_util::Full;
use hyper::Method;
use serde_derive::{Deserialize, Serialize};

use std::cmp::Eq;
use std::collections::HashMap;
use std::hash::Hash;

use super::Docker;
use crate::docker::BodyType;
use crate::errors::Error;

use crate::models::*;

/// Network configuration used in the [Create Network API](Docker::create_network())
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::models::NetworkCreateRequest"
)]
#[serde(rename_all = "PascalCase")]
pub struct CreateNetworkOptions<T>
where
    T: Into<String> + Eq + Hash,
{
    /// The network's name.
    pub name: T,
    /// Check for networks with duplicate names. Since Network is primarily keyed based on a random
    /// ID and not on the name, and network name is strictly a user-friendly alias to the network
    /// which is uniquely identified using ID, there is no guaranteed way to check for duplicates.
    /// CheckDuplicate is there to provide a best effort checking of any networks which has the
    /// same name but it is not guaranteed to catch all name collisions.
    pub check_duplicate: bool,
    /// Name of the network driver plugin to use.
    pub driver: T,
    /// Restrict external access to the network.
    pub internal: bool,
    /// Globally scoped network is manually attachable by regular containers from workers in swarm mode.
    pub attachable: bool,
    /// Ingress network is the network which provides the routing-mesh in swarm mode.
    pub ingress: bool,
    /// Controls IP address management when creating a network.
    #[serde(rename = "IPAM")]
    pub ipam: Ipam,
    /// Enable IPv6 on the network.
    #[serde(rename = "EnableIPv6")]
    pub enable_ipv6: bool,
    /// Network specific options to be used by the drivers.
    pub options: HashMap<T, T>,
    /// User-defined key/value metadata.
    pub labels: HashMap<T, T>,
}

impl<T> From<CreateNetworkOptions<T>> for NetworkCreateRequest
where
    T: Into<String> + Eq + Hash,
{
    fn from(opts: CreateNetworkOptions<T>) -> Self {
        NetworkCreateRequest {
            name: opts.name.into(),
            driver: Some(opts.driver.into()),
            internal: Some(opts.internal),
            attachable: Some(opts.attachable),
            ingress: Some(opts.ingress),
            ipam: Some(opts.ipam),
            enable_ipv6: Some(opts.enable_ipv6),
            options: Some(
                opts.options
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
            ),
            labels: Some(
                opts.labels
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
            ),
            ..Default::default()
        }
    }
}

/// Parameters used in the [Inspect Network API](super::Docker::inspect_network())
///
/// ## Examples
///
/// ```rust
/// use bollard::network::InspectNetworkOptions;
///
/// InspectNetworkOptions{
///     verbose: true,
///     scope: "global",
/// };
/// ```
///
/// ```rust
/// # use bollard::network::InspectNetworkOptions;
/// # use std::default::Default;
/// InspectNetworkOptions::<&str>{
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::InspectNetworkOptions and associated InspectNetworkOptionsBuilder"
)]
pub struct InspectNetworkOptions<T>
where
    T: Into<String> + serde::ser::Serialize,
{
    /// Detailed inspect output for troubleshooting.
    pub verbose: bool,
    /// Filter the network by scope (swarm, global, or local)
    pub scope: T,
}

impl<T> From<InspectNetworkOptions<T>> for crate::query_parameters::InspectNetworkOptions
where
    T: Into<String> + serde::ser::Serialize,
{
    fn from(opts: InspectNetworkOptions<T>) -> Self {
        crate::query_parameters::InspectNetworkOptionsBuilder::default()
            .verbose(opts.verbose)
            .scope(&opts.scope.into())
            .build()
    }
}

/// Parameters used in the [List Networks API](super::Docker::list_networks())
///
/// ## Examples
///
/// ```rust
/// use bollard::network::ListNetworksOptions;
///
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("label", vec!["maintainer=some_maintainer"]);
///
/// ListNetworksOptions{
///     filters
/// };
/// ```
///
/// ```rust
/// # use bollard::network::ListNetworksOptions;
/// # use std::default::Default;
///
/// ListNetworksOptions::<&str> {
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::ListNetworksOptions and associated ListNetworksOptionsBuilder"
)]
pub struct ListNetworksOptions<T>
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    /// JSON encoded value of the filters (a `map[string][]string`) to process on the networks list. Available filters:
    ///  - `driver=<driver-name>` Matches a network's driver.
    ///  - `id=<network-id>` Matches all or part of a network ID.
    ///  - `label=<key>` or `label=<key>=<value>` of a network label.
    ///  - `name=<network-name>` Matches all or part of a network name.
    ///  - `scope=["swarm"|"global"|"local"]` Filters networks by scope (`swarm`, `global`, or `local`).
    ///  - `type=["custom"|"builtin"]` Filters networks by type. The `custom` keyword returns all user-defined networks.
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

impl<T> From<ListNetworksOptions<T>> for crate::query_parameters::ListNetworksOptions
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    fn from(opts: ListNetworksOptions<T>) -> Self {
        crate::query_parameters::ListNetworksOptionsBuilder::default()
            .filters(
                &opts
                    .filters
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into_iter().map(T::into).collect()))
                    .collect(),
            )
            .build()
    }
}

/// Network configuration used in the [Connect Network API](Docker::connect_network())
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::ConnectNetworkOptions and associated ConnectNetworkOptionsBuilder"
)]
#[serde(rename_all = "PascalCase")]
pub struct ConnectNetworkOptions<T>
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    /// The ID or name of the container to connect to the network.
    pub container: T,
    /// Configuration for a network endpoint.
    pub endpoint_config: EndpointSettings,
}

impl<T> From<ConnectNetworkOptions<T>> for NetworkConnectRequest
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    fn from(opts: ConnectNetworkOptions<T>) -> Self {
        NetworkConnectRequest {
            container: Some(opts.container.into()),
            endpoint_config: Some(opts.endpoint_config),
        }
    }
}

/// Network configuration used in the [Disconnect Network API](Docker::disconnect_network())
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::DisconnectNetworkOptions and associated DisconnectNetworkOptionsBuilder"
)]
#[serde(rename_all = "PascalCase")]
pub struct DisconnectNetworkOptions<T>
where
    T: Into<String> + serde::ser::Serialize,
{
    /// The ID or name of the container to disconnect from the network.
    pub container: T,
    /// Force the container to disconnect from the network.
    pub force: bool,
}

impl<T> From<DisconnectNetworkOptions<T>> for NetworkDisconnectRequest
where
    T: Into<String> + serde::ser::Serialize,
{
    fn from(opts: DisconnectNetworkOptions<T>) -> Self {
        NetworkDisconnectRequest {
            container: Some(opts.container.into()),
            force: Some(opts.force),
        }
    }
}

/// Parameters used in the [Prune Networks API](Docker::prune_networks())
///
/// ## Examples
///
/// ```rust
/// use bollard::network::PruneNetworksOptions;
///
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("label!", vec!["maintainer=some_maintainer"]);
///
/// PruneNetworksOptions{
///     filters
/// };
/// ```
///
/// ```rust
/// # use bollard::network::PruneNetworksOptions;
/// # use std::default::Default;
///
/// PruneNetworksOptions::<&str>{
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::PruneNetworksOptions and associated PruneNetworksOptionsBuilder"
)]
pub struct PruneNetworksOptions<T>
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    /// Filters to process on the prune list, encoded as JSON.
    ///  - `until=<timestamp>` Prune networks created before this timestamp. The `<timestamp>` can be
    ///    Unix timestamps, date formatted timestamps, or Go duration strings (e.g. `10m`, `1h30m`)
    ///    computed relative to the daemon machine’s time.
    ///  - label (`label=<key>`, `label=<key>=<value>`, `label!=<key>`, or `label!=<key>=<value>`)
    ///    Prune networks with (or without, in case `label!=...` is used) the specified labels.
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

impl<T> From<PruneNetworksOptions<T>> for crate::query_parameters::PruneNetworksOptions
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    fn from(opts: PruneNetworksOptions<T>) -> Self {
        crate::query_parameters::PruneNetworksOptionsBuilder::default()
            .filters(
                &opts
                    .filters
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into_iter().map(T::into).collect()))
                    .collect(),
            )
            .build()
    }
}

impl Docker {
    /// ---
    ///
    /// # Create Network
    ///
    /// Create a new network.
    ///
    /// # Arguments
    ///
    ///  - [Create Network Options](CreateNetworkOptions) struct.
    ///
    /// # Returns
    ///
    ///  - A [Network Create Response](NetworkCreateResponse) struct, wrapped in a
    ///    Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::network::CreateNetworkOptions;
    ///
    /// use std::default::Default;
    ///
    /// let config = CreateNetworkOptions {
    ///     name: "certs",
    ///     ..Default::default()
    /// };
    ///
    /// docker.create_network(config);
    /// ```
    pub async fn create_network(
        &self,
        config: impl Into<NetworkCreateRequest>,
    ) -> Result<NetworkCreateResponse, Error> {
        let url = "/networks/create";

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
    /// # Remove a Network
    ///
    /// # Arguments
    ///
    ///  - Network name as a string slice.
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
    /// docker.remove_network("my_network_name");
    /// ```
    pub async fn remove_network(&self, network_name: &str) -> Result<(), Error> {
        let url = format!("/networks/{network_name}");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Inspect a Network
    ///
    /// # Arguments
    ///
    ///  - Network name as a string slice.
    ///
    /// # Returns
    ///
    ///  - A [Models](Network) struct, wrapped in a
    ///    Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::network::InspectNetworkOptions;
    ///
    /// use std::default::Default;
    ///
    /// let config = InspectNetworkOptions {
    ///     verbose: true,
    ///     scope: "global"
    /// };
    ///
    /// docker.inspect_network("my_network_name", Some(config));
    /// ```
    pub async fn inspect_network(
        &self,
        network_name: &str,
        options: Option<impl Into<crate::query_parameters::InspectNetworkOptions>>,
    ) -> Result<Network, Error> {
        let url = format!("/networks/{network_name}");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # List Networks
    ///
    /// # Arguments
    ///
    ///  - Optional [List Network Options](ListNetworksOptions) struct.
    ///
    /// # Returns
    ///
    ///  - A vector of [Network](Network) struct, wrapped in a
    ///    Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::network::ListNetworksOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut list_networks_filters = HashMap::new();
    /// list_networks_filters.insert("label", vec!["maintainer=some_maintainer"]);
    ///
    /// let config = ListNetworksOptions {
    ///     filters: list_networks_filters,
    /// };
    ///
    /// docker.list_networks(Some(config));
    /// ```
    pub async fn list_networks(
        &self,
        options: Option<impl Into<crate::query_parameters::ListNetworksOptions>>,
    ) -> Result<Vec<Network>, Error> {
        let url = "/networks";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Connect Network
    ///
    /// # Arguments
    ///
    ///  - A [Connect Network Options](ConnectNetworkOptions) struct.
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
    /// use bollard::network::ConnectNetworkOptions;
    /// use bollard::models::{EndpointSettings, EndpointIpamConfig};
    ///
    /// use std::default::Default;
    ///
    /// let config = ConnectNetworkOptions {
    ///     container: "3613f73ba0e4",
    ///     endpoint_config: EndpointSettings {
    ///         ipam_config: Some(EndpointIpamConfig {
    ///             ipv4_address: Some(String::from("172.24.56.89")),
    ///             ipv6_address: Some(String::from("2001:db8::5689")),
    ///             ..Default::default()
    ///         }),
    ///         ..Default::default()
    ///     }
    /// };
    ///
    /// docker.connect_network("my_network_name", config);
    /// ```
    pub async fn connect_network(
        &self,
        network_name: &str,
        config: impl Into<NetworkConnectRequest>,
    ) -> Result<(), Error> {
        let url = format!("/networks/{network_name}/connect");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
            Docker::serialize_payload(Some(config.into())),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Disconnect Network
    ///
    /// # Arguments
    ///
    ///  - A [Disconnect Network Options](DisconnectNetworkOptions) struct.
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
    /// use bollard::network::DisconnectNetworkOptions;
    ///
    /// use std::default::Default;
    ///
    /// let config = DisconnectNetworkOptions {
    ///     container: "3613f73ba0e4",
    ///     force: true
    /// };
    ///
    /// docker.disconnect_network("my_network_name", config);
    /// ```
    pub async fn disconnect_network(
        &self,
        network_name: &str,
        config: impl Into<NetworkDisconnectRequest>,
    ) -> Result<(), Error> {
        let url = format!("/networks/{network_name}/disconnect");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
            Docker::serialize_payload(Some(config.into())),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Prune Networks
    ///
    /// Deletes networks which are unused.
    ///
    /// # Arguments
    ///
    ///  - A [Prune Networks Options](PruneNetworksOptions) struct.
    ///
    /// # Returns
    ///
    ///  - A [Network Prune Response](NetworkPruneResponse) struct.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::network::PruneNetworksOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("label", vec!["maintainer=some_maintainer"]);
    ///
    /// let options = PruneNetworksOptions {
    ///     filters,
    /// };
    ///
    /// docker.prune_networks(Some(options));
    /// ```
    pub async fn prune_networks(
        &self,
        options: Option<impl Into<crate::query_parameters::PruneNetworksOptions>>,
    ) -> Result<NetworkPruneResponse, Error> {
        let url = "/networks/prune";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }
}
