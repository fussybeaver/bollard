//! Network API: Networks are user-defined networks that containers can be attached to.

use arrayvec::ArrayVec;
use http::request::Builder;
use hyper::rt::Future;
use hyper::{Body, Method};
use serde::ser::Serialize;
use serde_json;

use std::cmp::Eq;
use std::collections::HashMap;
use std::hash::Hash;

use super::{Docker, DockerChain};
#[cfg(test)]
use crate::docker::API_DEFAULT_VERSION;
use crate::docker::{FALSE_STR, TRUE_STR};
use crate::errors::Error;
use crate::errors::ErrorKind::JsonSerializeError;

/// Network configuration used in the [Create Network API](../struct.Docker.html#method.create_network)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateNetworkOptions<T>
where
    T: AsRef<str> + Eq + Hash,
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
    pub ipam: IPAM<T>,
    /// Enable IPv6 on the network.
    #[serde(rename = "EnableIPv6")]
    pub enable_ipv6: bool,
    /// Network specific options to be used by the drivers.
    pub options: HashMap<T, T>,
    /// User-defined key/value metadata.
    pub labels: HashMap<T, T>,
}

/// IPAM represents IP Address Management
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct IPAM<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Name of the IPAM driver to use.
    pub driver: T,
    /// List of IPAM configuration options, specified as a map: {"Subnet": <CIDR>, "IPRange": <CIDR>, "Gateway": <IP address>, "AuxAddress": <device_name:IP address>}
    pub config: Vec<IPAMConfig<T>>,
    /// Driver-specific options, specified as a map.
    pub options: Option<HashMap<T, T>>,
}

/// IPAMConfig represents IPAM configurations
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct IPAMConfig<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub subnet: Option<T>,
    #[serde(rename = "IPRange")]
    pub ip_range: Option<T>,
    pub gateway: Option<T>,
    pub aux_address: Option<HashMap<T, T>>,
}

/// Result type for the [Create Network API](../struct.Docker.html#method.create_network)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct CreateNetworkResults {
    pub id: String,
    pub warning: String,
}

/// Parameters used in the [Inspect Network API](../struct.Docker.html#method.inspect_network)
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InspectNetworkOptions<T> {
    /// Detailed inspect output for troubleshooting.
    pub verbose: bool,
    /// Filter the network by scope (swarm, global, or local)
    pub scope: T,
}

#[allow(missing_docs)]
/// Trait providing implementations for [Inspect Network Options](struct.InspectNetworkOptions.html)
/// struct.
pub trait InspectNetworkQueryParams<'a, V>
where
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(&'a str, V); 2]>, Error>;
}

impl<'a> InspectNetworkQueryParams<'a, &'a str> for InspectNetworkOptions<&'a str> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, &'a str); 2]>, Error> {
        Ok(ArrayVec::from([
            ("verbose", if self.verbose { TRUE_STR } else { FALSE_STR }),
            ("scope", self.scope),
        ]))
    }
}

impl<'a> InspectNetworkQueryParams<'a, String> for InspectNetworkOptions<String> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 2]>, Error> {
        Ok(ArrayVec::from([
            ("verbose", self.verbose.to_string()),
            ("scope", self.scope),
        ]))
    }
}

/// Result type for the [Inspect Network API](../struct.Docker.html#method.inspect_network)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct InspectNetworkResults {
    pub name: String,
    pub id: String,
    pub created: String,
    pub scope: String,
    pub driver: String,
    #[serde(rename = "EnableIPv6")]
    pub enable_ipv6: bool,
    #[serde(rename = "IPAM")]
    pub ipam: IPAM<String>,
    pub internal: bool,
    pub attachable: bool,
    pub ingress: bool,
    pub containers: HashMap<String, InspectNetworkResultsContainers>,
    pub options: HashMap<String, String>,
    pub labels: HashMap<String, String>,
    pub config_from: HashMap<String, String>,
    pub config_only: bool,
}

/// Result type for the [Inspect Network API](../struct.Docker.html#method.inspect_network)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct InspectNetworkResultsContainers {
    pub name: String,
    #[serde(rename = "EndpointID")]
    pub endpoint_id: String,
    pub mac_address: String,
    #[serde(rename = "IPv4Address")]
    pub ipv4_address: String,
    #[serde(rename = "IPv6Address")]
    pub ipv6_address: String,
}

/// Result type for the [List Networks API](../struct.Docker.html#method.list_networks)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ListNetworksResults {
    pub name: String,
    pub id: String,
    pub created: String,
    pub scope: String,
    pub driver: String,
    #[serde(rename = "EnableIPv6")]
    pub enable_ipv6: bool,
    pub internal: bool,
    pub attachable: bool,
    pub ingress: bool,
    #[serde(rename = "IPAM")]
    pub ipam: IPAM<String>,
    pub options: HashMap<String, String>,
    pub config_from: HashMap<String, String>,
    pub config_only: bool,
    pub containers: HashMap<String, InspectNetworkResultsContainers>,
    pub labels: HashMap<String, String>,
}

/// Parameters used in the [List Networks API](../struct.Docker.html#method.list_networks)
///
/// ## Examples
///
/// ```rust
/// use bollard::network::ListNetworksOptions;
///
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("label", vec!("maintainer=some_maintainer"));
///
/// ListNetworksOptions{
///     filters: filters
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListNetworksOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// JSON encoded value of the filters (a `map[string][]string`) to process on the networks list. Available filters:
    ///  - `driver=<driver-name>` Matches a network's driver.
    ///  - `id=<network-id>` Matches all or part of a network ID.
    ///  - `label=<key>` or `label=<key>=<value>` of a network label.
    ///  - `name=<network-name>` Matches all or part of a network name.
    ///  - `scope=["swarm"|"global"|"local"]` Filters networks by scope (`swarm`, `global`, or `local`).
    ///  - `type=["custom"|"builtin"]` Filters networks by type. The `custom` keyword returns all user-defined networks.
    pub filters: HashMap<T, Vec<T>>,
}

#[allow(missing_docs)]
/// Trait providing implementations for [List Networks Options](struct.ListNetworksOptions.html)
/// struct.
pub trait ListNetworksQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, String); 1]>, Error>;
}

impl<'a> ListNetworksQueryParams<&'a str, String> for ListNetworksOptions<&'a str> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 1]>, Error> {
        Ok(ArrayVec::from([(
            "filters",
            serde_json::to_string(&self.filters)
                .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
        )]))
    }
}

/// Network configuration used in the [Connect Network API](../struct.Docker.html#method.connect_network)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ConnectNetworkOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// The ID or name of the container to connect to the network.
    pub container: T,
    /// Configuration for a network endpoint.
    pub endpoint_config: EndpointSettings<T>,
}

/// Configuration for a network endpoint.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct EndpointSettings<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// EndpointIPAMConfig represents an endpoint's IPAM configuration.
    #[serde(rename = "IPAMConfig")]
    pub ipam_config: EndpointIPAMConfig<T>,
    #[allow(missing_docs)]
    pub links: Vec<T>,
    #[allow(missing_docs)]
    pub aliases: Vec<T>,
    /// Unique ID of the network.
    #[serde(rename = "NetworkID")]
    pub network_id: T,
    /// Unique ID for the service endpoint in a Sandbox.
    #[serde(rename = "EndpointID")]
    pub endpoint_id: T,
    /// Gateway address for this network.
    pub gateway: T,
    /// IPv4 address.
    #[serde(rename = "IPAddress")]
    pub ip_address: T,
    /// Mask length of the IPv4 address.
    #[serde(rename = "IPPrefixLen")]
    pub ip_prefix_len: isize,
    /// IPv6 gateway address.
    #[serde(rename = "IPv6Gateway")]
    pub ipv6_gateway: T,
    /// Global IPv6 address.
    #[serde(rename = "GlobalIPv6Address")]
    pub global_ipv6_address: T,
    /// Mask length of the global IPv6 address.
    #[serde(rename = "GlobalIPv6PrefixLen")]
    pub global_ipv6_prefix_len: i64,
    /// MAC address for the endpoint on this network.
    pub mac_address: T,
    /// DriverOpts is a mapping of driver options and values. These options are passed directly to
    /// the driver and are driver specific.
    pub driver_opts: Option<HashMap<T, T>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct EndpointIPAMConfig<T>
where
    T: AsRef<str>,
{
    #[serde(rename = "IPv4Address")]
    pub ipv4_address: T,
    #[serde(rename = "IPv6Address")]
    pub ipv6_address: T,
    #[serde(rename = "LinkLocalIPs")]
    pub link_local_ips: Vec<T>,
}

/// Network configuration used in the [Disconnect Network API](../struct.Docker.html#method.disconnect_network)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DisconnectNetworkOptions<T>
where
    T: AsRef<str>,
{
    /// The ID or name of the container to disconnect from the network.
    pub container: T,
    /// Force the container to disconnect from the network.
    pub force: bool,
}

/// Parameters used in the [Prune Networks API](../struct.Docker.html#method.prune_networks)
///
/// ## Examples
///
/// ```rust
/// use bollard::network::PruneNetworksOptions;
///
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("label!", vec!("maintainer=some_maintainer"));
///
/// PruneNetworksOptions{
///     filters: filters
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
#[derive(Debug, Clone, Default)]
pub struct PruneNetworksOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Filters to process on the prune list, encoded as JSON.
    ///  - `until=<timestamp>` Prune networks created before this timestamp. The `<timestamp>` can be
    ///  Unix timestamps, date formatted timestamps, or Go duration strings (e.g. `10m`, `1h30m`)
    ///  computed relative to the daemon machine’s time.  
    ///  - label (`label=<key>`, `label=<key>=<value>`, `label!=<key>`, or `label!=<key>=<value>`)
    ///  Prune networks with (or without, in case `label!=...` is used) the specified labels.
    pub filters: HashMap<T, Vec<T>>,
}

/// Trait providing implementations for [Prune Networks Options](struct.PruneNetworksOptions.html)
/// struct.
#[allow(missing_docs)]
pub trait PruneNetworksQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, String); 1]>, Error>;
}

impl<'a> PruneNetworksQueryParams<&'a str, String> for PruneNetworksOptions<&'a str> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 1]>, Error> {
        Ok(ArrayVec::from([(
            "filters",
            serde_json::to_string(&self.filters)
                .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
        )]))
    }
}

/// Result type for the [Prune Networks API](../struct.Docker.html#method.prune_networks)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct PruneNetworksResults {
    pub networks_deleted: Option<Vec<String>>,
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
    ///  - [Create Network Options](network/struct.CreateNetworkOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A [Create Network Results](network/struct.CreateNetworkResults.html) struct, wrapped in a
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
    pub fn create_network<T>(
        &self,
        config: CreateNetworkOptions<T>,
    ) -> impl Future<Item = CreateNetworkResults, Error = Error>
    where
        T: AsRef<str> + Eq + Hash + Serialize,
    {
        let url = "/networks/create";

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::POST),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_value(req)
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
    pub fn remove_network(&self, network_name: &str) -> impl Future<Item = (), Error = Error> {
        let url = format!("/networks/{}", network_name);

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::DELETE),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_unit(req)
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
    ///  - A [Inspect Network Results](network/struct.CreateNetworkResults.html) struct, wrapped in a
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
    pub fn inspect_network<'a, T, V>(
        &self,
        network_name: &str,
        options: Option<T>,
    ) -> impl Future<Item = InspectNetworkResults, Error = Error>
    where
        T: InspectNetworkQueryParams<'a, V>,
        V: AsRef<str>,
    {
        let url = format!("/networks/{}", network_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
    }

    /// ---
    ///
    /// # List Networks
    ///
    /// # Arguments
    ///
    ///  - Optional [List Network Options](network/struct.ListNetworksOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A vector of [List Networks Results](network/struct.CreateNetworkResults.html) struct, wrapped in a
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
    pub fn list_networks<T, K, V>(
        &self,
        options: Option<T>,
    ) -> impl Future<Item = Vec<ListNetworksResults>, Error = Error>
    where
        T: ListNetworksQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = "/networks";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
    }

    /// ---
    ///
    /// # Connect Network
    ///
    /// # Arguments
    ///
    ///  - A [Connect Network Options](network/struct.ConnectNetworkOptions.html) struct.
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
    /// use bollard::network::{EndpointSettings, EndpointIPAMConfig, ConnectNetworkOptions};
    ///
    /// use std::default::Default;
    ///
    /// let config = ConnectNetworkOptions {
    ///     container: "3613f73ba0e4",
    ///     endpoint_config: EndpointSettings {
    ///         ipam_config: EndpointIPAMConfig {
    ///             ipv4_address: "172.24.56.89",
    ///             ipv6_address: "2001:db8::5689",
    ///             ..Default::default()
    ///         },
    ///         ..Default::default()
    ///     }
    /// };
    ///
    /// docker.connect_network("my_network_name", config);
    /// ```
    pub fn connect_network<T>(
        &self,
        network_name: &str,
        config: ConnectNetworkOptions<T>,
    ) -> impl Future<Item = (), Error = Error>
    where
        T: AsRef<str> + Eq + Hash + Serialize,
    {
        let url = format!("/networks/{}/connect", network_name);

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::POST),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_unit(req)
    }

    /// ---
    ///
    /// # Disconnect Network
    ///
    /// # Arguments
    ///
    ///  - A [Disconnect Network Options](network/struct.DisconnectNetworkOptions.html) struct.
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
    pub fn disconnect_network<T>(
        &self,
        network_name: &str,
        config: DisconnectNetworkOptions<T>,
    ) -> impl Future<Item = (), Error = Error>
    where
        T: AsRef<str> + Serialize,
    {
        let url = format!("/networks/{}/disconnect", network_name);

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::POST),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_unit(req)
    }

    /// ---
    ///
    /// # Prune Networks
    ///
    /// Deletes networks which are unused.
    ///
    /// # Arguments
    ///
    ///  - A [Prune Networks Options](network/struct.PruneNetworksOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A [Prune Networks Results](network/struct.PruneNetworksResults.html) struct.
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
    /// filters.insert("label", vec!("maintainer=some_maintainer"));
    ///
    /// let options = PruneNetworksOptions {
    ///     filters: filters,
    /// };
    ///
    /// docker.prune_networks(Some(options));
    /// ```
    pub fn prune_networks<T, K, V>(
        &self,
        options: Option<T>,
    ) -> impl Future<Item = PruneNetworksResults, Error = Error>
    where
        T: PruneNetworksQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = "/networks/prune";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
    }
}

impl DockerChain {
    /// ---
    ///
    /// # Create Network
    ///
    /// Create a new network. Consumes the client instance.
    ///
    /// # Arguments
    ///
    ///  - [Create Network Options](container/struct.CreateNetworkOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a
    ///  [Create Exec Results](container/struct.CreateNetworkResults.html) struct, wrapped in a
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
    /// docker.chain().create_network(config);
    /// ```
    pub fn create_network<T>(
        self,
        config: CreateNetworkOptions<T>,
    ) -> impl Future<Item = (DockerChain, CreateNetworkResults), Error = Error>
    where
        T: AsRef<str> + Eq + Hash + Serialize,
    {
        self.inner
            .create_network(config)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Remove a Network
    ///
    /// Remove an existing network. Consumes the client instance.
    ///
    /// # Arguments
    ///
    ///  - Network name as a string slice.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.chain().remove_network("my_network_name");
    /// ```
    pub fn remove_network(
        self,
        network_name: &str,
    ) -> impl Future<Item = (DockerChain, ()), Error = Error> {
        self.inner
            .remove_network(network_name)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Inspect a Network
    ///
    /// # Arguments
    ///
    ///  - Network name as a string slice. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a
    ///  [Inspect Network Results](container/struct.CreateNetworkResults.html) struct, wrapped in a
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
    /// docker.chain().inspect_network("my_network_name", Some(config));
    /// ```
    pub fn inspect_network<'a, T, V>(
        self,
        network_name: &str,
        options: Option<T>,
    ) -> impl Future<Item = (DockerChain, InspectNetworkResults), Error = Error>
    where
        T: InspectNetworkQueryParams<'a, V>,
        V: AsRef<str>,
    {
        self.inner
            .inspect_network(network_name, options)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # List Networks
    ///
    /// # Arguments
    ///
    ///  - Optional [List Network Options](container/struct.ListNetworksOptions.html) struct.
    ///  Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a vector
    ///  of [List Networks Results](container/struct.CreateNetworkResults.html) struct, wrapped in
    ///  a Future.
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
    /// docker.chain().list_networks(Some(config));
    /// ```
    pub fn list_networks<T, K, V>(
        self,
        options: Option<T>,
    ) -> impl Future<Item = (DockerChain, Vec<ListNetworksResults>), Error = Error>
    where
        T: ListNetworksQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.inner
            .list_networks(options)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Connect Network
    ///
    /// # Arguments
    ///
    ///  - A [Connect Network Options](network/struct.ConnectNetworkOptions.html) struct. Consumes
    ///  the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::network::{EndpointSettings, EndpointIPAMConfig, ConnectNetworkOptions};
    ///
    /// use std::default::Default;
    ///
    /// let config = ConnectNetworkOptions {
    ///     container: "3613f73ba0e4",
    ///     endpoint_config: EndpointSettings {
    ///         ipam_config: EndpointIPAMConfig {
    ///             ipv4_address: "172.24.56.89",
    ///             ipv6_address: "2001:db8::5689",
    ///             ..Default::default()
    ///         },
    ///         ..Default::default()
    ///     }
    /// };
    ///
    /// docker.chain().connect_network("my_network_name", config);
    /// ```
    pub fn connect_network<T>(
        self,
        network_name: &str,
        config: ConnectNetworkOptions<T>,
    ) -> impl Future<Item = (DockerChain, ()), Error = Error>
    where
        T: AsRef<str> + Eq + Hash + Serialize,
    {
        self.inner
            .connect_network(network_name, config)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Disconnect Network
    ///
    /// # Arguments
    ///
    ///  - A [Disconnect Network Options](network/struct.DisconnectNetworkOptions.html) struct.
    ///  Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a unit
    ///  type `()`, wrapped in a Future.
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
    /// docker.chain().disconnect_network("my_network_name", config);
    /// ```
    pub fn disconnect_network<T>(
        self,
        network_name: &str,
        config: DisconnectNetworkOptions<T>,
    ) -> impl Future<Item = (DockerChain, ()), Error = Error>
    where
        T: AsRef<str> + Serialize,
    {
        self.inner
            .disconnect_network(network_name, config)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Prune Networks
    ///
    /// Deletes networks which are unused. Consumes the client instance.
    ///
    /// # Arguments
    ///
    ///  - A [Prune Networks Options](network/struct.PruneNetworksOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a [Prune
    ///  Networks Results](network/struct.PruneNetworksResults.html) struct.
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
    /// filters.insert("label", vec!("maintainer=some_maintainer"));
    ///
    /// let options = PruneNetworksOptions {
    ///     filters: filters,
    /// };
    ///
    /// docker.chain().prune_networks(Some(options));
    /// ```
    pub fn prune_networks<T, K, V>(
        self,
        options: Option<T>,
    ) -> impl Future<Item = (DockerChain, PruneNetworksResults), Error = Error>
    where
        T: PruneNetworksQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.inner
            .prune_networks(options)
            .map(|result| (self, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyper_mock::HostToReplyConnector;
    use tokio::runtime::Runtime;

    #[test]
    fn test_create_network() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();

        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 86\r\n\r\n{\"Id\":\"d1022f34e396473dd2a1e39abe0816b6e3465cdb44b78d094606f122933d8da3\",\"Warning\":\"\"}\r\n".to_string()
        );

        let docker =
            Docker::connect_with_host_to_reply(connector, "_".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let ipam_config = IPAMConfig {
            subnet: Some("10.10.10.10/24"),
            ..Default::default()
        };

        let create_network_options = CreateNetworkOptions {
            name: "integration_test_create_network",
            check_duplicate: true,
            ipam: IPAM {
                config: vec![ipam_config],
                ..Default::default()
            },
            ..Default::default()
        };

        let results = docker.create_network(create_network_options);

        let future = results.map(|result| {
            assert_eq!(
                "d1022f34e396473dd2a1e39abe0816b6e3465cdb44b78d094606f122933d8da3".to_string(),
                result.id
            )
        });

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_inspect_network() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();

        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 428\r\n\r\n{\"Name\":\"integration_test_create_network\",\"Id\":\"c1c29f1f4265b413cad016eddb2ce25b9c55a59d7e1241f8a6c9bfa55b865a43\",\"Created\":\"2019-02-23T09:23:10.60267Z\",\"Scope\":\"local\",\"Driver\":\"bridge\",\"EnableIPv6\":false,\"IPAM\":{\"Driver\":\"default\",\"Options\":null,\"Config\":[{\"Subnet\":\"10.10.10.10/24\"}]},\"Internal\":false,\"Attachable\":false,\"Ingress\":false,\"ConfigFrom\":{\"Network\":\"\"},\"ConfigOnly\":false,\"Containers\":{},\"Options\":{},\"Labels\":{}}\r\n".to_string()
        );

        let docker =
            Docker::connect_with_host_to_reply(connector, "_".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let results = docker.inspect_network(
            "c1c29f1f4265b413cad016eddb2ce25b9c55a59d7e1241f8a6c9bfa55b865a43",
            Some(InspectNetworkOptions {
                verbose: true,
                scope: "global",
            }),
        );

        let future = results.map(|result| {
            assert!(result
                .ipam
                .config
                .into_iter()
                .take(1)
                .any(|i| i.subnet.unwrap() == "10.10.10.10/24"));
        });

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_remove_network() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();

        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker =
            Docker::connect_with_host_to_reply(connector, "_".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let results = docker.remove_network("my_network_name");

        let future = results.map(|_| assert!(true));

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_list_networks() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();

        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 463\r\n\r\n[{\"Name\":\"integration_test_list_network\",\"Id\":\"594423d1fbc3d894f8dc5a5b6565fffe4b4b5c379ceb23e1daf6ead4e3397fe3\",\"Created\":\"2019-02-23T09:54:21.8154268Z\",\"Scope\":\"local\",\"Driver\":\"bridge\",\"EnableIPv6\":false,\"IPAM\":{\"Driver\":\"default\",\"Options\":null,\"Config\":[{\"Subnet\":\"10.10.10.10/24\"}]},\"Internal\":false,\"Attachable\":false,\"Ingress\":false,\"ConfigFrom\":{\"Network\":\"\"},\"ConfigOnly\":false,\"Containers\":{},\"Options\":{},\"Labels\":{\"maintainer\":\"bollard-maintainer\"}}]\r\n".to_string()
        );

        let docker =
            Docker::connect_with_host_to_reply(connector, "_".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let mut list_networks_filters = HashMap::new();
        list_networks_filters.insert("label", vec!["maintainer=bollard-maintainer"]);

        let results = docker.list_networks(Some(ListNetworksOptions {
            filters: list_networks_filters,
        }));

        let future = results.map(|result| {
            assert!(result
                .into_iter()
                .take(1)
                .map(|v| v.ipam.config)
                .flatten()
                .any(|i| i.subnet.unwrap() == "10.10.10.10/24"))
        });

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_connect_network() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();

        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker =
            Docker::connect_with_host_to_reply(connector, "_".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let connect_network_options = ConnectNetworkOptions {
            container: "my_running_container",
            endpoint_config: EndpointSettings {
                ipam_config: EndpointIPAMConfig {
                    ipv4_address: "10.10.10.101",
                    ..Default::default()
                },
                ..Default::default()
            },
        };

        let results = docker.connect_network(
            "f3f9ef4375ca3ada374b9ecd6d8a1ebd501a59f0b2eedd0b93cd0502d7a009dc",
            connect_network_options,
        );

        let future = results.map(|_| assert!(true));

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_disconnect_network() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();

        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker =
            Docker::connect_with_host_to_reply(connector, "_".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let results = docker.disconnect_network(
            "f3f9ef4375ca3ada374b9ecd6d8a1ebd501a59f0b2eedd0b93cd0502d7a009dc",
            DisconnectNetworkOptions {
                container: "my_running_container",
                force: true,
            },
        );

        let future = results.map(|_| assert!(true));

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_prune_networks() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();

        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 44\r\n\r\n{\"NetworksDeleted\":[\"my_running_container\"]}\r\n".to_string()
        );

        let docker =
            Docker::connect_with_host_to_reply(connector, "_".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let results = docker.prune_networks(None::<PruneNetworksOptions<&str>>);

        let future =
            results.map(|v| assert_eq!("my_running_container", v.networks_deleted.unwrap()[0]));

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }
}
