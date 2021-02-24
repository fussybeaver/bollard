//! Network API: Networks are user-defined networks that containers can be attached to.

use http::request::Builder;
use hyper::{Body, Method};
use serde::ser::Serialize;

use std::cmp::Eq;
use std::collections::HashMap;
use std::hash::Hash;

use super::Docker;
use crate::errors::Error;

use crate::models::*;

/// Network configuration used in the [Create Network API](Docker::create_network())
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct InspectNetworkOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Detailed inspect output for troubleshooting.
    pub verbose: bool,
    /// Filter the network by scope (swarm, global, or local)
    pub scope: T,
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct ListNetworksOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
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

/// Network configuration used in the [Connect Network API](Docker::connect_network())
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ConnectNetworkOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
{
    /// The ID or name of the container to connect to the network.
    pub container: T,
    /// Configuration for a network endpoint.
    pub endpoint_config: EndpointSettings,
}

/// Network configuration used in the [Disconnect Network API](Docker::disconnect_network())
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DisconnectNetworkOptions<T>
where
    T: Into<String> + Serialize,
{
    /// The ID or name of the container to disconnect from the network.
    pub container: T,
    /// Force the container to disconnect from the network.
    pub force: bool,
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct PruneNetworksOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
{
    /// Filters to process on the prune list, encoded as JSON.
    ///  - `until=<timestamp>` Prune networks created before this timestamp. The `<timestamp>` can be
    ///  Unix timestamps, date formatted timestamps, or Go duration strings (e.g. `10m`, `1h30m`)
    ///  computed relative to the daemon machine’s time.
    ///  - label (`label=<key>`, `label=<key>=<value>`, `label!=<key>`, or `label!=<key>=<value>`)
    ///  Prune networks with (or without, in case `label!=...` is used) the specified labels.
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
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
    ///  Future.
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
    pub async fn create_network<T>(
        &self,
        config: CreateNetworkOptions<T>,
    ) -> Result<NetworkCreateResponse, Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/networks/create";

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
        let url = format!("/networks/{}", network_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            None::<String>,
            Ok(Body::empty()),
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
    ///  Future.
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
    pub async fn inspect_network<T>(
        &self,
        network_name: &str,
        options: Option<InspectNetworkOptions<T>>,
    ) -> Result<Network, Error>
    where
        T: Into<String> + Serialize,
    {
        let url = format!("/networks/{}", network_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options,
            Ok(Body::empty()),
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
    ///  Future.
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
    pub async fn list_networks<T>(
        &self,
        options: Option<ListNetworksOptions<T>>,
    ) -> Result<Vec<Network>, Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/networks";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options,
            Ok(Body::empty()),
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
    pub async fn connect_network<T>(
        &self,
        network_name: &str,
        config: ConnectNetworkOptions<T>,
    ) -> Result<(), Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = format!("/networks/{}/connect", network_name);

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
    pub async fn disconnect_network<T>(
        &self,
        network_name: &str,
        config: DisconnectNetworkOptions<T>,
    ) -> Result<(), Error>
    where
        T: Into<String> + Serialize,
    {
        let url = format!("/networks/{}/disconnect", network_name);

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
    pub async fn prune_networks<T>(
        &self,
        options: Option<PruneNetworksOptions<T>>,
    ) -> Result<NetworkPruneResponse, Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/networks/prune";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options,
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }
}
