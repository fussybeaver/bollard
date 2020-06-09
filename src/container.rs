//! Container API: run docker containers and manage their lifecycle

use arrayvec::ArrayVec;
use chrono::serde::ts_seconds;
use chrono::{DateTime, Utc};
use futures_core::Stream;
use http::header::CONTENT_TYPE;
use http::request::Builder;
use hyper::{body::Bytes, Body, Method};
use serde::Serialize;
use serde_json;

use std::cmp::Eq;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;

use super::Docker;
use crate::docker::{FALSE_STR, TRUE_STR};
use crate::errors::Error;
use crate::errors::ErrorKind::JsonSerializeError;
use crate::network::EndpointIPAMConfig;

/// Parameters used in the [List Container API](../struct.Docker.html#method.list_containers)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::ListContainersOptions;
///
/// use std::collections::HashMap;
/// use std::default::Default;
///
/// let mut filters = HashMap::new();
/// filters.insert("health", vec!("unhealthy"));
///
/// ListContainersOptions{
///     all: true,
///     filters: filters,
///     ..Default::default()
/// };
/// ```
///
/// ```rust
/// # use bollard::container::ListContainersOptions;
/// # use std::default::Default;
/// ListContainersOptions::<String>{
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct ListContainersOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Return all containers. By default, only running containers are shown
    pub all: bool,
    /// Return this number of most recently created containers, including non-running ones
    pub limit: Option<isize>,
    /// Return the size of container as fields `SizeRw` and `SizeRootFs`
    pub size: bool,
    /// Filters to process on the container list, encoded as JSON. Available filters:
    ///  - `ancestor`=`(<image-name>[:<tag>]`, `<image id>`, or `<image@digest>`)
    ///  - `before`=(`<container id>` or `<container name>`)
    ///  - `expose`=(`<port>[/<proto>]`|`<startport-endport>`/`[<proto>]`)
    ///  - `exited`=`<int>` containers with exit code of `<int>`
    ///  - `health`=(`starting`|`healthy`|`unhealthy`|`none`)
    ///  - `id`=`<ID>` a container's ID
    ///  - `isolation`=(`default`|`process`|`hyperv`) (Windows daemon only)
    ///  - `is-task`=`(true`|`false`)
    ///  - `label`=`key` or `label`=`"key=value"` of a container label
    ///  - `name`=`<name>` a container's name
    ///  - `network`=(`<network id>` or `<network name>`)
    ///  - `publish`=(`<port>[/<proto>]`|`<startport-endport>`/`[<proto>]`)
    ///  - `since`=(`<container id>` or `<container name>`)
    ///  - `status`=(`created`|`restarting`|`running`|`removing`|`paused`|`exited`|`dead`)
    ///  - `volume`=(`<volume name>` or `<mount point destination>`)
    pub filters: HashMap<T, Vec<T>>,
}

#[allow(missing_docs)]
/// Trait providing implementations for [List Containers Options](struct.ListContainersOptions.html)
/// struct.
pub trait ListContainersQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 4]>, Error>;
}

impl<'a, T: AsRef<str> + Eq + Hash> ListContainersQueryParams<&'a str, String>
    for ListContainersOptions<T>
where
    T: ::serde::Serialize,
{
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 4]>, Error> {
        Ok(ArrayVec::from([
            ("all", self.all.to_string()),
            (
                "limit",
                self.limit
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| String::new()),
            ),
            ("size", self.size.to_string()),
            (
                "filters",
                serde_json::to_string(&self.filters).map_err(|e| JsonSerializeError { err: e })?,
            ),
        ]))
    }
}

/// Parameters used in the [Create Container API](../struct.Docker.html#method.create_container)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::CreateContainerOptions;
///
/// CreateContainerOptions{
///     name: "my-new-container",
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct CreateContainerOptions<T>
where
    T: AsRef<str>,
{
    /// Assign the specified name to the container.
    pub name: T,
}

/// Trait providing implementations for [Create Container Options](struct.CreateContainerOptions.html)
/// struct.
#[allow(missing_docs)]
pub trait CreateContainerQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a, T: AsRef<str>> CreateContainerQueryParams<&'a str, T> for CreateContainerOptions<T> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, T); 1]>, Error> {
        Ok(ArrayVec::from([("name", self.name)]))
    }
}

/// A request for devices to be sent to device drivers
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct DeviceRequest<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub driver: T,
    pub count: i64,
    #[serde(rename = "DeviceIDs")]
    pub device_ids: Vec<T>,
    /// A list of capabilities; an OR list of AND lists of capabilities.
    pub capabilities: Vec<T>,
    /// Driver-specific options, specified as a key/value pairs. These options are passed directly to the driver.
    pub options: Option<HashMap<T, T>>,
}

/// Bind options for mounts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MountPointBindOptions<T>
where
    T: AsRef<str>,
{
    /// A propagation mode with the value `[r]private`, `[r]shared`, or `[r]slave`.
    pub propagation: T,
    /// Disable recursive bind mount.
    pub non_recursive: bool,
}

/// Driver config for volume options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VolumeOptionsDriverConfig<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Name of the driver to use to create the volume.
    pub name: T,
    /// key/value map of driver specific options.
    pub options: HashMap<T, T>,
}

/// Volume options for mounts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MountPointVolumeOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Populate volume with data from the target.
    pub no_copy: bool,
    /// User-defined key/value metadata.
    pub labels: HashMap<T, T>,
    /// Map of driver specific options
    pub driver_config: VolumeOptionsDriverConfig<T>,
}

/// Tmpfs options for mounts.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MountPointTmpfsOptions {
    /// The size for the tmpfs mount in bytes.
    pub size_bytes: u64,
    /// The permission mode for the tmpfs mount in an integer.
    pub mode: usize,
}

/// Specification for mounts to be added to the container.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct MountPoint<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Container path.
    pub target: T,
    /// Mount source (e.g. a volume name, a host path).
    pub source: T,
    /// The mount type. Available types:
    ///   - `bind` Mounts a file or directory from the host into the container. Must exist prior to creating the container.
    ///   - `volume` Creates a volume with the given name and options (or uses a pre-existing volume with the same name and options). These are **not** removed when the container is removed.
    ///   - `tmpfs` Create a tmpfs with the given options. The mount source cannot be specified for tmpfs.
    ///   - `npipe` Mounts a named pipe from the host into the container. Must exist prior to creating the container.
    #[serde(rename = "Type")]
    pub type_: T,
    /// Whether the mount should be read-only.
    pub read_only: Option<bool>,
    /// The consistency requirement for the mount: `default`, `consistent`, `cached`, or `delegated`.
    pub consistency: Option<T>,
    /// Optional configuration for the `bind` type.
    pub bind_options: Option<MountPointBindOptions<T>>,
    /// Optional configuration for the `volume` type.
    pub volume_options: Option<MountPointVolumeOptions<T>>,
    /// Optional configuration for the `tmpfs` type.
    pub tmpfs_options: Option<MountPointTmpfsOptions>,
}

/// Ulimit definitions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct Ulimits {
    pub name: String,
    pub hard: Option<u64>,
    pub soft: Option<u64>,
}

/// Container configuration that depends on the host we are running on
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct HostConfig<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// A list of volume bindings for this container. Each volume binding is a string in one of these forms:
    /// - `host-src:container-dest` to bind-mount a host path into the container. Both `host-src`, and `container-dest` must be an *absolute* path.
    /// - `host-src:container-dest:ro` to make the bind mount read-only inside the container. Both `host-src`, and `container-dest` must be an *absolute* path.
    /// - `volume-name:container-dest` to bind-mount a volume managed by a volume driver into the container. `container-dest` must be an *absolute* path.
    /// - `volume-name:container-dest:ro` to mount the volume read-only inside the container. `container-dest` must be an *absolute* path.
    pub binds: Option<Vec<T>>,
    /// A list of links for the container in the form `container_name:alias`.
    pub links: Option<Vec<T>>,
    /// Memory limit in bytes.
    pub memory: Option<u64>,
    /// Total memory limit (memory + swap). Set as `-1` to enable unlimited swap.
    pub memory_swap: Option<i64>,
    /// Memory soft limit in bytes.
    pub memory_reservation: Option<u64>,
    /// Kernel memory limit in bytes.
    pub kernel_memory: Option<u64>,
    /// Hard limit for kernel TCP buffer memory (in bytes).
    #[serde(rename = "KernelMemoryTCP")]
    pub kernel_memory_tcp: Option<i64>,
    /// CPU quota in units of 10<sup>-9</sup> CPUs.
    pub nano_cpus: Option<u64>,
    pub cpu_percent: Option<u64>,
    /// An integer value representing this container's relative CPU weight versus other containers.
    pub cpu_shares: Option<u64>,
    /// The length of a CPU period in microseconds.
    pub cpu_period: Option<u64>,
    /// The length of a CPU real-time period in microseconds. Set to 0 to allocate no time allocated to real-time tasks.
    pub cpu_realtime_period: Option<u64>,
    /// The length of a CPU real-time runtime in microseconds. Set to 0 to allocate no time allocated to real-time tasks.
    pub cpu_realtime_runtime: Option<u64>,
    /// Microseconds of CPU time that the container can get in a CPU period.
    pub cpu_quota: Option<u64>,
    /// CPUs in which to allow execution (e.g., `0-3`, `0,1`)
    pub cpuset_cpus: Option<T>,
    /// Memory nodes (MEMs) in which to allow execution (`0-3`, `0,1`). Only effective on NUMA systems.
    pub cpuset_mems: Option<T>,
    /// Block IO weight (relative weight).
    pub blkio_weight: Option<u64>,
    /// Block IO weight (relative device weight).
    pub blkio_weight_device: Option<Vec<HashMap<T, T>>>,
    /// Limit read rate (bytes per second) from a device.
    pub blkio_device_read_bps: Option<Vec<HashMap<T, T>>>,
    /// Limit write rate (bytes per second) to a device.
    pub blkio_device_write_bps: Option<Vec<HashMap<T, T>>>,
    /// Limit read rate (IO per second) from a device.
    #[serde(rename = "BlkioDeviceReadIOps")]
    pub blkio_device_read_iops: Option<Vec<HashMap<T, T>>>,
    /// Limit write rate (IO per second) to a device.
    #[serde(rename = "BlkioDeviceWriteIOps")]
    pub blkio_device_write_iops: Option<Vec<HashMap<T, T>>>,
    /// Tune a container's memory swappiness behavior. Accepts an integer between 0 and 100.
    pub memory_swappiness: Option<i64>,
    /// Disable OOM Killer for the container.
    pub oom_kill_disable: Option<bool>,
    /// An integer value containing the score given to the container in order to tune OOM killer
    /// preferences.
    pub oom_score_adj: Option<isize>,
    /// Set the PID (Process) Namespace mode for the container. It can be either:
    /// - `"container:<name|id>"`: joins another container's PID namespace
    /// - `"host"`: use the host's PID namespace inside the container
    pub pid_mode: Option<String>,
    /// Tune a container's pids limit. Set `-1` for unlimited.
    pub pids_limit: Option<u64>,
    /// PortMap describes the mapping of container ports to host ports, using the container's
    /// port-number and protocol as key in the format `<port>/<protocol`>, for example, `80/udp`.  If a
    /// container's port is mapped for multiple protocols, separate entries are added to the
    /// mapping table.
    pub port_bindings: Option<HashMap<T, Vec<PortBinding<T>>>>,
    /// Allocates an ephemeral host port for all of a container's exposed ports.
    /// Ports are de-allocated when the container stops and allocated when the container starts.
    /// The allocated port might be changed when restarting the container.
    /// The port is selected from the ephemeral port range that depends on the kernel. For example,
    /// on Linux the range is defined by `/proc/sys/net/ipv4/ip_local_port_range`.
    pub publish_all_ports: Option<bool>,
    /// Gives the container full access to the host.
    pub privileged: Option<bool>,
    /// Mount the container's root filesystem as read only.
    pub readonly_rootfs: Option<bool>,
    /// A list of DNS servers for the container to use.
    pub dns: Option<Vec<T>>,
    /// A list of DNS options.
    pub dns_options: Option<Vec<T>>,
    /// A list of DNS search domains.
    pub dns_search: Option<Vec<T>>,
    /// A list of volumes to inherit from another container, specified in the form `<container
    /// name>[:<ro|rw>]`.
    pub volumes_from: Option<Vec<T>>,
    /// Specification for mounts to be added to the container.
    pub mounts: Option<Vec<MountPoint<T>>>,
    /// A list of kernel capabilities to be available for container (this overrides the default set).  Conflicts with options 'CapAdd' and 'CapDrop'
    pub capabilities: Option<Vec<T>>,
    /// A list of kernel capabilities to add to the container. Conflicts with option 'Capabilities'
    pub cap_add: Option<Vec<T>>,
    /// A list of kernel capabilities to drop from the container. Conflicts with option 'Capabilities'
    pub cap_drop: Option<Vec<T>>,
    pub group_add: Option<Vec<T>>,
    /// The behavior to apply when the container exits. The default is not to restart.
    /// An ever increasing delay (double the previous delay, starting at 100ms) is added before
    /// each restart to prevent flooding the server.
    pub restart_policy: Option<RestartPolicy<T>>,
    /// Automatically remove the container when the container's process exits. This has no effect
    /// if `RestartPolicy` is set.
    pub auto_remove: Option<bool>,
    /// Network mode to use for this container. Supported standard values are: `bridge`, `host`,
    /// `none`, and `container:<name|id>`. Any other value is taken as a custom network's name to
    /// which this container should connect to.
    pub network_mode: Option<T>,
    pub devices: Option<Vec<HashMap<T, T>>>,
    /// A list of resource limits to set in the container. For example: `{"Name": "nofile", "Soft":
    /// 1024, "Hard": 2048}`
    pub ulimits: Option<Vec<Ulimits>>,
    /// The logging configuration for this container.
    pub log_config: Option<LogConfig>,
    /// A list of string values to customize labels for MLS systems, such as SELinux.
    pub security_opt: Option<Vec<T>>,
    /// Path to `cgroups` under which the container's `cgroup` is created. If the path is not absolute,
    /// the path is considered to be relative to the `cgroups` path of the init process. Cgroups are
    /// created if they do not already exist.
    pub cgroup_parent: Option<T>,
    /// Driver that this container uses to mount volumes.
    pub volume_driver: Option<T>,
    /// Size of `/dev/shm` in bytes. If omitted, the system uses 64MB.
    pub shm_size: Option<u64>,
    /// Path to a file where the container ID is written.
    #[serde(rename = "ContainerIDFile")]
    pub container_id_file: Option<String>,
    /// A list of hostnames/IP mappings to add to the container's `/etc/hosts` file. Specified in
    /// the form `["hostname:IP"]`.
    pub extra_hosts: Option<Vec<T>>,
    /// IPC sharing mode for the container. Possible values are:
    ///  - `"none"`: own private IPC namespace, with /dev/shm not mounted
    ///  - `"private"`: own private IPC namespace
    ///  - `"shareable"`: own private IPC namespace, with a possibility to share it with other containers
    ///  - `"container:<name|id>"`: join another (shareable) container's IPC namespace
    ///  - `"host"`: use the host system's IPC namespace
    /// If not specified, daemon default is used, which can either be "private" or "shareable",
    /// depending on daemon version and configuration.
    pub ipc_mode: Option<T>,
    /// Cgroup to use for the container.
    pub cgroup: Option<T>,
    /// UTS namespace to use for the container.
    #[serde(rename = "UTSMode")]
    pub uts_mode: Option<T>,
    /// Sets the usernamespace mode for the container when usernamespace remapping option is enabled.
    pub userns_mode: Option<T>,
    /// Runtime to use with this container.
    pub runtime: Option<T>,
    /// Initial console size, as an [height, width] array. (Windows only)
    pub console_size: Option<Vec<isize>>,
    /// Isolation technology of the container. (Windows only)
    pub isolation: Option<T>,
    /// A list of cgroup rules to apply to the container.
    pub device_cgroup_rules: Option<Vec<T>>,
    /// Disk limit (in bytes).
    pub disk_quota: Option<u64>,
    /// A list of requests for devices to be sent to device drivers
    pub device_requests: Option<DeviceRequest<T>>,
    /// Hard limit for kernel TCP buffer memory (in bytes).
    pub kernet_memory_tcp: Option<i64>,
    /// The usable percentage of the available CPUs (Windows only).
    /// On Windows Server containers, the processor resource controls are mutually exclusive. The
    /// order of precedence is `CPUCount` first, then `CPUShares`, and `CPUPercent` last.
    pub cpu_count: Option<u64>,
    /// Maximum IOps for the container system drive (Windows only).
    #[serde(rename = "IOMaximumIOps")]
    pub io_maximum_iops: Option<u64>,
    /// Maximum IO in bytes per second for the container system drive (Windows only).
    #[serde(rename = "IOMaximumBandwidth")]
    pub io_maximum_bandwidth: Option<u64>,
    pub masked_paths: Option<Vec<T>>,
    pub readonly_paths: Option<Vec<T>>,
    pub sysctls: Option<HashMap<T, T>>,
}

/// Storage driver name and configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct GraphDriver {
    pub name: String,
    pub data: Option<HashMap<String, String>>,
}

/// Describes the mapping of container ports to host ports, using the container's
/// port-number and protocol as key in the format `<port>/<protocol>`, for example, `80/udp`.  If a
/// container's port is mapped for multiple protocols, separate entries are added to the mapping
/// table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct PortBinding<T>
where
    T: AsRef<str>,
{
    #[serde(rename = "HostIp")]
    pub host_ip: T,
    pub host_port: T,
}

/// The behavior to apply when the container exits. The default is not to restart.  An ever
/// increasing delay (double the previous delay, starting at 100ms) is added before each restart to
/// prevent flooding the server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct RestartPolicy<T>
where
    T: AsRef<str>,
{
    pub name: Option<T>,
    pub maximum_retry_count: Option<isize>,
}

/// The logging configuration for this container.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct LogConfig {
    #[serde(rename = "Type")]
    pub type_: Option<String>,
    pub config: Option<HashMap<String, String>>,
}

/// This container's networking configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct NetworkingConfig {
    pub endpoints_config: HashMap<String, ContainerNetwork>,
}

/// Configuration for a network endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ContainerNetwork {
    #[serde(rename = "IPAMConfig")]
    pub ipam_config: Option<EndpointIPAMConfig<String>>,
    pub links: Option<Vec<String>>,
    pub aliases: Option<Vec<String>>,
    pub mac_address: String,
    #[serde(rename = "GlobalIPv6Address")]
    pub global_ipv6_address: String,
    #[serde(rename = "GlobalIPv6PrefixLen")]
    pub global_ipv6_prefix_len: isize,
    #[serde(rename = "IPv6Gateway")]
    pub ipv6_gateway: String,
    #[serde(rename = "IPAddress")]
    pub ip_address: String,
    #[serde(rename = "IPPrefixLen")]
    pub ip_prefix_len: i64,
    pub gateway: String,
    #[serde(rename = "EndpointID")]
    pub endpoint_id: String,
    #[serde(rename = "NetworkID")]
    pub network_id: String,
    pub driver_opts: Option<HashMap<String, String>>,
}

/// Network Settings for a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct NetworkSettings {
    pub networks: HashMap<String, ContainerNetwork>,
    #[serde(rename = "IPAddress")]
    pub ip_address: String,
    #[serde(rename = "IPPrefixLen")]
    pub ip_prefix_len: isize,
    pub mac_address: String,
    pub gateway: String,
    pub bridge: String,
    #[serde(rename = "EndpointID")]
    pub endpoint_id: String,
    pub sandbox_key: String,
    #[serde(rename = "GlobalIPv6Address")]
    pub global_ipv6_address: String,
    #[serde(rename = "GlobalIPv6PrefixLen")]
    pub global_ipv6_prefix_len: isize,
    #[serde(rename = "IPv6Gateway")]
    pub ipv6_gateway: String,
    #[serde(rename = "LinkLocalIPv6Address")]
    pub link_local_ipv6_address: String,
    #[serde(rename = "LinkLocalIPv6PrefixLen")]
    pub link_local_ipv6_prefix_len: isize,
    #[serde(rename = "SecondaryIPAddresses")]
    pub secondary_ip_addresses: Option<Vec<String>>,
    #[serde(rename = "SecondaryIPv6Addresses")]
    pub secondary_ipv6_addresses: Option<Vec<String>>,
    #[serde(rename = "SandboxID")]
    pub sandbox_id: String,
    pub hairpin_mode: bool,
    pub ports: HashMap<String, Option<Vec<PortBinding<String>>>>,
}

/// Specification for mounts to be added to the container.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct Mount {
    pub name: Option<String>,
    pub source: String,
    pub destination: String,
    pub driver: Option<String>,
    pub mode: String,
    #[serde(rename = "RW")]
    pub rw: bool,
    #[serde(rename = "Type")]
    pub type_: String,
    pub propagation: String,
}

/// Log of the health of a running container.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct LogStateHealth {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub exit_code: i16,
    pub output: String,
}

/// Health of a running container.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct StateHealth {
    pub status: String,
    pub failing_streak: u64,
    pub log: Vec<LogStateHealth>,
}

/// Runtime status of the container.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct State {
    pub status: String,
    pub running: bool,
    pub paused: bool,
    pub restarting: bool,
    #[serde(rename = "OOMKilled")]
    pub oomkilled: bool,
    pub dead: bool,
    pub pid: isize,
    pub exit_code: u16,
    pub error: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub health: Option<StateHealth>,
}

/// Maps internal container port to external host port.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct APIPort {
    #[serde(rename = "IP")]
    pub ip: Option<String>,
    pub private_port: i64,
    pub public_port: Option<i64>,
    #[serde(rename = "Type")]
    pub type_: String,
}

/// A mapping of network name to endpoint configuration for that network.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct NetworkList {
    pub networks: HashMap<String, ContainerNetwork>,
}

/// Result type for the [List Containers API](../struct.Docker.html#method.list_containers)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct APIContainers {
    pub id: String,
    pub names: Vec<String>,
    pub image: String,
    #[serde(rename = "ImageID")]
    pub image_id: String,
    pub command: String,
    #[serde(with = "ts_seconds")]
    pub created: DateTime<Utc>,
    pub state: String,
    pub status: String,
    pub ports: Vec<APIPort>,
    pub labels: HashMap<String, String>,
    pub size_rw: Option<i64>,
    pub size_root_fs: Option<i64>,
    pub mounts: Vec<Mount>,
    pub network_settings: NetworkList,
    pub host_config: HostConfig<String>,
}

/// Result type for the [Inspect Container API](../struct.Docker.html#method.inspect_container)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct Container {
    pub id: String,
    pub created: DateTime<Utc>,
    pub path: String,
    pub args: Vec<String>,
    pub config: Config<String>,
    pub state: State,
    pub image: String,
    pub network_settings: NetworkSettings,
    pub resolv_conf_path: String,
    pub hostname_path: String,
    pub hosts_path: String,
    pub log_path: String,
    pub name: String,
    pub driver: String,
    pub mounts: Vec<Mount>,
    pub host_config: HostConfig<String>,
    pub restart_count: isize,
    pub platform: String,
    pub mount_label: String,
    pub process_label: String,
    pub app_armor_profile: String,
    #[serde(rename = "ExecIDs")]
    pub exec_ids: Option<Vec<String>>,
    pub graph_driver: GraphDriver,
}

/// A test to perform to check that the container is healthy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HealthConfig {
    /// The test to perform. Possible values are:
    ///  - `[]` inherit healthcheck from image or parent image
    ///  - `["NONE"]` disable healthcheck
    ///  - `["CMD", args...]` exec arguments directly
    ///  - `["CMD-SHELL", command]` run command with system's default shell
    pub test: Option<Vec<String>>,
    /// The time to wait between checks in nanoseconds. It should be 0 or at least 1000000 (1 ms).
    /// 0 means inherit.
    pub interval: Option<u64>,
    /// The time to wait before considering the check to have hung. It should be 0 or at least
    /// 1000000 (1 ms). 0 means inherit.
    pub timeout: Option<u64>,
    /// The number of consecutive failures needed to consider a container as unhealthy. 0 means
    /// inherit.
    pub retries: Option<u64>,
    /// Start period for the container to initialize before starting health-retries countdown in
    /// nanoseconds. It should be 0 or at least 1000000 (1 ms). 0 means inherit.
    pub start_period: Option<u64>,
}

/// Container to create.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Config<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// The hostname to use for the container, as a valid RFC 1123 hostname.
    pub hostname: Option<T>,
    /// The domain name to use for the container.
    pub domainname: Option<T>,
    /// The user that commands are run as inside the container.
    pub user: Option<T>,
    /// Whether to attach to `stdin`.
    pub attach_stdin: Option<bool>,
    /// Whether to attach to `stdout`.
    pub attach_stdout: Option<bool>,
    /// Whether to attach to `stderr`.
    pub attach_stderr: Option<bool>,
    /// Command is already escaped (Windows only).
    pub args_escaped: Option<bool>,
    /// Attach standard streams to a TTY, including `stdin` if it is not closed.
    pub tty: Option<bool>,
    /// Open `stdin`.
    pub open_stdin: Option<bool>,
    /// Close stdin after one attached client disconnects.
    pub stdin_once: Option<bool>,
    /// A list of environment variables to set inside the container in the form `["VAR=value", ...]`.
    /// A variable without `=` is removed from the environment, rather than to have an empty value.
    pub env: Option<Vec<T>>,
    /// Command to run specified as a string or an array of strings.
    pub cmd: Option<Vec<T>>,
    /// The entry point for the container as a string or an array of strings.
    ///
    /// If the array consists of exactly one empty string (`[""]`) then the entry point is reset to
    /// system default (i.e., the entry point used by docker when there is no `ENTRYPOINT`
    /// instruction in the `Dockerfile`).
    pub entrypoint: Option<Vec<T>>,
    /// The name of the image to use when creating the container.
    pub image: Option<T>,
    /// User-defined key/value metadata.
    pub labels: Option<HashMap<T, T>>,
    /// An object mapping mount point paths inside the container to empty objects.
    pub volumes: Option<HashMap<T, HashMap<(), ()>>>,
    /// The working directory for commands to run in.
    pub working_dir: Option<T>,
    /// Disable networking for the container.
    pub network_disabled: Option<bool>,
    /// `ONBUILD` metadata that were defined in the image's `Dockerfile`.
    pub on_build: Option<Vec<T>>,
    /// MAC address of the container.
    pub mac_address: Option<T>,
    /// An object mapping ports to an empty object in the form:
    /// `{"<port>/<tcp|udp|sctp>": {}}`
    pub exposed_ports: Option<HashMap<T, HashMap<(), ()>>>,
    /// Signal to stop a container as a string or unsigned integer.
    pub stop_signal: Option<T>,
    /// Timeout to stop a container in seconds.
    pub stop_timeout: Option<isize>,
    /// Container configuration that depends on the host we are running on.
    pub host_config: Option<HostConfig<T>>,
    /// This container's networking configuration.
    pub networking_config: Option<NetworkingConfig>,
    /// A test to perform to check that the container is healthy.
    pub healthcheck: Option<HealthConfig>,
}

/// Result type for the [Create Container API](../struct.Docker.html#method.create_container)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct CreateContainerResults {
    pub id: String,
    pub warnings: Option<Vec<String>>,
}

/// Parameters used in the [Stop Container API](../struct.Docker.html#method.stop_container)
///
/// ## Examples
///
/// use bollard::container::StopContainerOptions;
///
/// StopContainerOptions{
///     t: 30,
/// };
#[derive(Debug, Copy, Clone, Default)]
pub struct StopContainerOptions {
    /// Number of seconds to wait before killing the container
    pub t: i64,
}

/// Trait providing implementations for [Stop Container Options](struct.StopContainerOptions.html).
#[allow(missing_docs)]
pub trait StopContainerQueryParams<K>
where
    K: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, String); 1]>, Error>;
}

impl<'a> StopContainerQueryParams<&'a str> for StopContainerOptions {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 1]>, Error> {
        Ok(ArrayVec::from([("t", self.t.to_string())]))
    }
}

/// Parameters used in the [Start Container API](../struct.Docker.html#method.start_container)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::StartContainerOptions;
///
/// StartContainerOptions{
///     detach_keys: "ctrl-^"
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct StartContainerOptions<T>
where
    T: AsRef<str>,
{
    /// Override the key sequence for detaching a container. Format is a single character `[a-Z]` or
    /// `ctrl-<value>` where `<value>` is one of: `a-z`, `@`, `^`, `[`, `,` or `_`.
    pub detach_keys: T,
}

/// Trait providing implementations for [Start Container Options](struct.StartContainerOptions.html).
#[allow(missing_docs)]
pub trait StartContainerQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a, T: AsRef<str>> StartContainerQueryParams<&'a str, T> for StartContainerOptions<T> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, T); 1]>, Error> {
        Ok(ArrayVec::from([("detachKeys", self.detach_keys)]))
    }
}

/// Parameters used in the [Remove Container API](../struct.Docker.html#method.remove_container)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::RemoveContainerOptions;
///
/// use std::default::Default;
///
/// RemoveContainerOptions{
///     force: true,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Copy, Clone, Default)]
pub struct RemoveContainerOptions {
    /// Remove the volumes associated with the container.
    pub v: bool,
    /// If the container is running, kill it before removing it.
    pub force: bool,
    /// Remove the specified link associated with the container.
    pub link: bool,
}

/// Trait providing implementations for [Remove Container Options](struct.RemoveContainerOptions.html).
#[allow(missing_docs)]
pub trait RemoveContainerQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 3]>, Error>;
}

impl<'a> RemoveContainerQueryParams<&'a str, &'a str> for RemoveContainerOptions {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, &'a str); 3]>, Error> {
        Ok(ArrayVec::from([
            ("v", if self.v { TRUE_STR } else { FALSE_STR }),
            ("force", if self.force { TRUE_STR } else { FALSE_STR }),
            ("link", if self.link { TRUE_STR } else { FALSE_STR }),
        ]))
    }
}

/// Parameters used in the [Wait Container API](../struct.Docker.html#method.wait_container)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::WaitContainerOptions;
///
/// WaitContainerOptions{
///     condition: "not-running",
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct WaitContainerOptions<T>
where
    T: AsRef<str>,
{
    /// Wait until a container state reaches the given condition, either 'not-running' (default),
    /// 'next-exit', or 'removed'.
    pub condition: T,
}

/// Trait providing implementations for [Wait Container Options](struct.WaitContainerOptions.html).
#[allow(missing_docs)]
pub trait WaitContainerQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a, T: AsRef<str>> WaitContainerQueryParams<&'a str, T> for WaitContainerOptions<T> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, T); 1]>, Error> {
        Ok(ArrayVec::from([("condition", self.condition)]))
    }
}

/// Error messages returned in the [Wait Container API](../struct.Docker.html#method.wait_container)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct WaitContainerResultsError {
    pub message: String,
}

/// Result type for the [Wait Container API](../struct.Docker.html#method.wait_container)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct WaitContainerResults {
    pub status_code: u64,
    pub error: Option<WaitContainerResultsError>,
}

/// Parameters used in the [Restart Container API](../struct.Docker.html#method.restart_container)
///
/// ## Example
///
/// ```rust
/// use bollard::container::RestartContainerOptions;
///
/// RestartContainerOptions{
///     t: 30,
/// };
/// ```
#[derive(Debug, Copy, Clone, Default)]
pub struct RestartContainerOptions {
    /// Number of seconds to wait before killing the container.
    pub t: isize,
}

/// Trait providing implementations for [Restart Container Options](struct.RestartContainerOptions.html).
#[allow(missing_docs)]
pub trait RestartContainerQueryParams<K>
where
    K: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, String); 1]>, Error>;
}

impl<'a> RestartContainerQueryParams<&'a str> for RestartContainerOptions {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 1]>, Error> {
        Ok(ArrayVec::from([("t", self.t.to_string())]))
    }
}

/// Parameters used in the [Inspect Container API](../struct.Docker.html#method.inspect_container)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::InspectContainerOptions;
///
/// InspectContainerOptions{
///     size: false,
/// };
/// ```
#[derive(Debug, Copy, Clone, Default)]
pub struct InspectContainerOptions {
    /// Return the size of container as fields `SizeRw` and `SizeRootFs`
    pub size: bool,
}

/// Trait providing implementations for [Inspect Container Options](struct.InspectContainerOptions.html).
#[allow(missing_docs)]
pub trait InspectContainerQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a> InspectContainerQueryParams<&'a str, &'a str> for InspectContainerOptions {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, &'a str); 1]>, Error> {
        Ok(ArrayVec::from([(
            "size",
            if self.size { TRUE_STR } else { FALSE_STR },
        )]))
    }
}

/// Parameters used in the [Top Processes API](../struct.Docker.html#method.top_processes)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::TopOptions;
///
/// TopOptions{
///     ps_args: "aux",
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct TopOptions<T>
where
    T: AsRef<str>,
{
    /// The arguments to pass to `ps`. For example, `aux`
    pub ps_args: T,
}

/// ## Top Query Params
///
/// Trait providing implementations for [Top Options](struct.TopOptions.html).
#[allow(missing_docs)]
pub trait TopQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a, T: AsRef<str>> TopQueryParams<&'a str, T> for TopOptions<T> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, T); 1]>, Error> {
        Ok(ArrayVec::from([("ps_args", self.ps_args)]))
    }
}

/// Result type for the [Top Processes API](../struct.Docker.html#method.top_processes)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TopResult {
    pub titles: Vec<String>,
    pub processes: Option<Vec<Vec<String>>>,
}

/// Parameters used in the [Logs API](../struct.Docker.html#method.logs)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::LogsOptions;
///
/// use std::default::Default;
///
/// LogsOptions{
///     stdout: true,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct LogsOptions {
    /// Return the logs as a finite stream.
    pub follow: bool,
    /// Return logs from `stdout`.
    pub stdout: bool,
    /// Return logs from `stderr`.
    pub stderr: bool,
    /// Only return logs since this time, as a UNIX timestamp.
    pub since: i64,
    /// Only return logs before this time, as a UNIX timestamp.
    pub until: i64,
    /// Add timestamps to every log line.
    pub timestamps: bool,
    /// Only return this number of log lines from the end of the logs. Specify as an integer or all
    /// to output `all` log lines.
    pub tail: String,
}

/// Trait providing implementations for [Logs Options](struct.LogsOptions.html).
#[allow(missing_docs)]
pub trait LogsQueryParams<K>
where
    K: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, String); 7]>, Error>;
}

impl<'a> LogsQueryParams<&'a str> for LogsOptions {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 7]>, Error> {
        Ok(ArrayVec::from([
            ("follow", self.follow.to_string()),
            ("stdout", self.stdout.to_string()),
            ("stderr", self.stderr.to_string()),
            ("since", self.since.to_string()),
            ("until", self.until.to_string()),
            ("timestamps", self.timestamps.to_string()),
            ("tail", self.tail),
        ]))
    }
}

/// Result type for the [Logs API](../struct.Docker.html#method.logs)
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub enum LogOutput {
    StdErr { message: String },
    StdOut { message: String },
    StdIn { message: String },
    Console { message: String },
}

impl fmt::Display for LogOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            LogOutput::StdErr { message } => write!(f, "{}", message),
            LogOutput::StdOut { message } => write!(f, "{}", message),
            LogOutput::StdIn { message } => write!(f, "{}", message),
            LogOutput::Console { message } => write!(f, "{}", message),
        }
    }
}

/// Result type for the [Container Changes API](../struct.Docker.html#method.container_changes)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct Change {
    pub path: String,
    pub kind: isize,
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            0 => write!(f, "C {}", self.path),
            1 => write!(f, "A {}", self.path),
            2 => write!(f, "D {}", self.path),
            _ => unreachable!(),
        }
    }
}

/// Parameters used in the [Stats API](../struct.Docker.html#method.stats)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::StatsOptions;
///
/// StatsOptions{
///     stream: false,
/// };
/// ```
#[derive(Debug, Copy, Clone, Default)]
pub struct StatsOptions {
    /// Stream the output. If false, the stats will be output once and then it will disconnect.
    pub stream: bool,
}

/// Trait providing implementations for [Stats Options](struct.StatsOptions.html).
#[allow(missing_docs)]
pub trait StatsQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a> StatsQueryParams<&'a str, &'a str> for StatsOptions {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, &'a str); 1]>, Error> {
        Ok(ArrayVec::from([(
            "stream",
            if self.stream { TRUE_STR } else { FALSE_STR },
        )]))
    }
}

/// Granular memory statistics for the container.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct MemoryStatsStats {
    pub cache: u64,
    pub dirty: u64,
    pub mapped_file: u64,
    pub total_inactive_file: u64,
    pub pgpgout: u64,
    pub rss: u64,
    pub total_mapped_file: u64,
    pub writeback: u64,
    pub unevictable: u64,
    pub pgpgin: u64,
    pub total_unevictable: u64,
    pub pgmajfault: u64,
    pub total_rss: u64,
    pub total_rss_huge: u64,
    pub total_writeback: u64,
    pub total_inactive_anon: u64,
    pub rss_huge: u64,
    pub hierarchical_memory_limit: u64,
    pub total_pgfault: u64,
    pub total_active_file: u64,
    pub active_anon: u64,
    pub total_active_anon: u64,
    pub total_pgpgout: u64,
    pub total_cache: u64,
    pub total_dirty: u64,
    pub inactive_anon: u64,
    pub active_file: u64,
    pub pgfault: u64,
    pub inactive_file: u64,
    pub total_pgmajfault: u64,
    pub total_pgpgin: u64,
    pub hierarchical_memsw_limit: Option<u64>,
}

/// General memory statistics for the container.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct MemoryStats {
    pub stats: Option<MemoryStatsStats>,
    pub max_usage: Option<u64>,
    pub usage: Option<u64>,
    pub failcnt: Option<u64>,
    pub limit: Option<u64>,
    pub commit: Option<u64>,
    pub commit_peak: Option<u64>,
    pub commitbytes: Option<u64>,
    pub commitpeakbytes: Option<u64>,
    pub privateworkingset: Option<u64>,
}

/// Process ID statistics for the container.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct PidsStats {
    pub current: Option<u64>,
    pub limit: Option<u64>,
}

/// I/O statistics for the container.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct BlkioStats {
    pub io_service_bytes_recursive: Option<Vec<BlkioStatsEntry>>,
    pub io_serviced_recursive: Option<Vec<BlkioStatsEntry>>,
    pub io_queue_recursive: Option<Vec<BlkioStatsEntry>>,
    pub io_service_time_recursive: Option<Vec<BlkioStatsEntry>>,
    pub io_wait_time_recursive: Option<Vec<BlkioStatsEntry>>,
    pub io_merged_recursive: Option<Vec<BlkioStatsEntry>>,
    pub io_time_recursive: Option<Vec<BlkioStatsEntry>>,
    pub sectors_recursive: Option<Vec<BlkioStatsEntry>>,
}

/// File I/O statistics for the container.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct StorageStats {
    pub read_count_normalized: Option<u64>,
    pub read_size_bytes: Option<u64>,
    pub write_count_normalized: Option<u64>,
    pub write_size_bytes: Option<u64>,
}

/// Statistics for the container.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct Stats {
    pub read: DateTime<Utc>,
    pub preread: DateTime<Utc>,
    pub num_procs: u32,
    pub pids_stats: PidsStats,
    pub network: Option<NetworkStats>,
    pub networks: Option<HashMap<String, NetworkStats>>,
    pub memory_stats: MemoryStats,
    pub blkio_stats: BlkioStats,
    pub cpu_stats: CPUStats,
    pub precpu_stats: CPUStats,
    pub storage_stats: StorageStats,
    pub name: String,
    pub id: String,
}

/// Network statistics for the container.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct NetworkStats {
    pub rx_dropped: u64,
    pub rx_bytes: u64,
    pub rx_errors: u64,
    pub tx_packets: u64,
    pub tx_dropped: u64,
    pub rx_packets: u64,
    pub tx_errors: u64,
    pub tx_bytes: u64,
}

/// CPU usage statistics for the container.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct CPUUsage {
    pub percpu_usage: Option<Vec<u64>>,
    pub usage_in_usermode: u64,
    pub total_usage: u64,
    pub usage_in_kernelmode: u64,
}

/// CPU throttling statistics.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct ThrottlingData {
    pub periods: u64,
    pub throttled_periods: u64,
    pub throttled_time: u64,
}

/// General CPU statistics for the container.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct CPUStats {
    pub cpu_usage: CPUUsage,
    pub system_cpu_usage: Option<u64>,
    pub online_cpus: Option<u64>,
    pub throttling_data: ThrottlingData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct BlkioStatsEntry {
    pub major: u64,
    pub minor: u64,
    pub op: String,
    pub value: u64,
}

/// Parameters used in the [Kill Container API](../struct.Docker.html#method.kill_container)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::KillContainerOptions;
///
/// KillContainerOptions{
///     signal: "SIGINT",
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct KillContainerOptions<T>
where
    T: AsRef<str>,
{
    /// Signal to send to the container as an integer or string (e.g. `SIGINT`)
    pub signal: T,
}

/// Trait providing implementations for [Kill Container Options](struct.KillContainerOptions.html).
#[allow(missing_docs)]
pub trait KillContainerQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a, T: AsRef<str>> KillContainerQueryParams<&'a str, T> for KillContainerOptions<T> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, T); 1]>, Error> {
        Ok(ArrayVec::from([("signal", self.signal)]))
    }
}

/// Block IO weight (relative device weight).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct UpdateContainerOptionsBlkioWeight {
    pub path: String,
    pub weight: isize,
}

/// Limit read/write rate (IO/bytes per second) from/to a device.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct UpdateContainerOptionsBlkioDeviceRate {
    pub path: String,
    pub rate: u64,
}

/// A list of devices to add to the container.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct UpdateContainerOptionsDevices {
    pub path_on_host: String,
    pub path_in_container: String,
    pub cgroup_permissions: String,
}

/// A list of resource limits to set in the container.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct UpdateContainerOptionsUlimits {
    pub name: String,
    pub soft: isize,
    pub hard: isize,
}

/// The behavior to apply when the container exits. The default is not to restart.
///
/// An ever increasing delay (double the previous delay, starting at 100ms) is added before each
/// restart to prevent flooding the server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct UpdateContainerOptionsRestartPolicy {
    pub name: String,
    pub maximum_retry_count: isize,
}

/// Configuration for the [Update Container API](../struct.Docker.html#method.update_container)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::UpdateContainerOptions;
/// use std::default::Default;
///
/// UpdateContainerOptions {
///     memory: Some(314572800),
///     memory_swap: Some(314572800),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateContainerOptions {
    /// An integer value representing this container's relative CPU weight versus other containers.
    pub cpu_shares: Option<isize>,
    /// Memory limit in bytes.
    pub memory: Option<u64>,
    /// Path to `cgroups` under which the container's `cgroup` is created. If the path is not absolute,
    /// the path is considered to be relative to the `cgroups` path of the init process. Cgroups are
    /// created if they do not already exist.
    pub cgroup_parent: Option<String>,
    /// Block IO weight (relative weight).
    pub blkio_weight: Option<isize>,
    /// Block IO weight (relative device weight).
    pub blkio_weight_device: Vec<UpdateContainerOptionsBlkioWeight>,
    /// Limit read rate (bytes per second) from a device.
    pub blkio_device_read_bps: Vec<UpdateContainerOptionsBlkioDeviceRate>,
    /// Limit write rate (bytes per second) to a device.
    #[serde(rename = "BlkioDeviceWriteIOps")]
    pub blkio_device_write_iops: Vec<UpdateContainerOptionsBlkioDeviceRate>,
    /// Limit read rate (IO per second) from a device.
    #[serde(rename = "BlkioDeviceReadIOps")]
    pub blkio_device_read_iops: Vec<UpdateContainerOptionsBlkioDeviceRate>,
    /// The length of a CPU period in microseconds.
    pub cpu_period: Option<u64>,
    /// Microseconds of CPU time that the container can get in a CPU period.
    pub cpu_quota: Option<u64>,
    /// The length of a CPU real-time period in microseconds. Set to 0 to allocate no time allocated to real-time tasks.
    pub cpu_realtime_period: Option<u64>,
    /// The length of a CPU real-time runtime in microseconds. Set to 0 to allocate no time allocated to real-time tasks.
    pub cpu_realtime_runtime: Option<u64>,
    /// CPUs in which to allow execution (e.g., `0-3`, `0,1`)
    pub cpuset_cpus: Option<String>,
    /// Memory nodes (MEMs) in which to allow execution (0-3, 0,1). Only effective on NUMA systems.
    pub cpuset_mems: Option<String>,
    /// A list of devices to add to the container.
    pub devices: Option<Vec<UpdateContainerOptionsDevices>>,
    /// A list of cgroup rules to apply to the container.
    pub device_cgroup_rules: Option<Vec<String>>,
    /// Disk limit (in bytes).
    pub disk_quota: Option<u64>,
    /// A list of requests for devices to be sent to device drivers
    pub device_requests: Option<DeviceRequest<String>>,
    /// Kernel memory limit in bytes.
    pub kernel_memory: Option<u64>,
    /// Hard limit for kernel TCP buffer memory (in bytes).
    #[serde(rename = "KernelMemoryTCP")]
    pub kernel_memory_tcp: Option<i64>,
    /// Memory soft limit in bytes.
    pub memory_reservation: Option<u64>,
    /// Total memory limit (memory + swap). Set as `-1` to enable unlimited swap.
    pub memory_swap: Option<i64>,
    /// Tune a container's memory swappiness behavior. Accepts an integer between 0 and 100.
    pub memory_swappiness: Option<i64>,
    /// CPU quota in units of 10<sup>-9</sup> CPUs.
    pub nano_cpus: Option<u64>,
    /// Disable OOM Killer for the container.
    pub oom_kill_disable: Option<bool>,
    /// Run an init inside the container that forwards signals and reaps processes. This field is
    /// omitted if empty, and the default (as configured on the daemon) is used.
    pub init: Option<bool>,
    /// Tune a container's PIDs limit. Set `0` or `-1` for unlimited, or `null` to not change.
    pub pids_limit: Option<u64>,
    /// A list of resource limits to set in the container.
    pub ulimits: Vec<UpdateContainerOptionsUlimits>,
    /// The number of usable CPUs (Windows only).
    ///
    /// On Windows Server containers, the processor resource controls are mutually exclusive. The
    /// order of precedence is `CPUCount` first, then `CPUShares`, and `CPUPercent` last.
    pub cpu_count: Option<u64>,
    /// The usable percentage of the available CPUs (Windows only).
    ///
    /// On Windows Server containers, the processor resource controls are mutually exclusive. The
    /// order of precedence is `CPUCount` first, then `CPUShares`, and `CPUPercent` last.
    pub cpu_percent: Option<u64>,
    /// Maximum IOps for the container system drive (Windows only).
    #[serde(rename = "IOMaximumIOps")]
    pub io_maximum_iops: Option<u64>,
    /// Maximum IO in bytes per second for the container system drive (Windows only).
    #[serde(rename = "IOMaximumBandwidth")]
    pub io_maximum_bandwidth: Option<u64>,
    /// The behavior to apply when the container exits. The default is not to restart.
    ///
    /// An ever increasing delay (double the previous delay, starting at 100ms) is added before
    /// each restart to prevent flooding the server.
    pub restart_policy: Option<UpdateContainerOptionsRestartPolicy>,
}

/// Parameters used in the [Rename Container API](../struct.Docker.html#method.rename_container)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::RenameContainerOptions;
///
/// RenameContainerOptions {
///     name: "my_new_container_name"
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct RenameContainerOptions<T>
where
    T: AsRef<str>,
{
    /// New name for the container.
    pub name: T,
}

/// Trait providing implementations for [Rename Container Options](struct.RenameContainerOptions.html).
#[allow(missing_docs)]
pub trait RenameContainerQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a, T: AsRef<str>> RenameContainerQueryParams<&'a str, T> for RenameContainerOptions<T> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, T); 1]>, Error> {
        Ok(ArrayVec::from([("name", self.name)]))
    }
}

/// Parameters used in the [Prune Containers API](../struct.Docker.html#method.prune_containers)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::PruneContainersOptions;
///
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("until", vec!("10m"));
///
/// PruneContainersOptions{
///     filters: filters
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct PruneContainersOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Filters to process on the prune list, encoded as JSON.
    ///
    /// Available filters:
    ///  - `until=<timestamp>` Prune containers created before this timestamp. The `<timestamp>` can be Unix timestamps, date formatted timestamps, or Go duration strings (e.g. `10m`, `1h30m`) computed relative to the daemon machine's time.
    ///  - label (`label=<key>`, `label=<key>=<value>`, `label!=<key>`, or `label!=<key>=<value>`) Prune containers with (or without, in case `label!=...` is used) the specified labels.
    pub filters: HashMap<T, Vec<T>>,
}

/// Trait providing implementations for [Prune Containers Options](struct.PruneContainersOptions.html).
#[allow(missing_docs)]
pub trait PruneContainersQueryParams<K>
where
    K: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, String); 1]>, Error>;
}

impl<'a, T: AsRef<str> + Eq + Hash + Serialize> PruneContainersQueryParams<&'a str>
    for PruneContainersOptions<T>
{
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 1]>, Error> {
        Ok(ArrayVec::from([(
            "filters",
            serde_json::to_string(&self.filters)
                .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
        )]))
    }
}

/// Result type for the [Prune Containers API](../struct.Docker.html#method.prune_containers)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct PruneContainersResults {
    pub containers_deleted: Option<Vec<String>>,
    pub space_reclaimed: u64,
}

/// Parameters used in the [Upload To Container
/// API](../struct.Docker.html#method.upload_to_container)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::UploadToContainerOptions;
///
/// use std::default::Default;
///
/// UploadToContainerOptions{
///     path: "/opt",
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct UploadToContainerOptions<T>
where
    T: AsRef<str>,
{
    /// Path to a directory in the container to extract the archive’s contents into.
    pub path: T,
    /// If “1”, “true”, or “True” then it will be an error if unpacking the given content would
    /// cause an existing directory to be replaced with a non-directory and vice versa.
    pub no_overwrite_dir_non_dir: T,
}

/// Trait providing implementations for [Upload To Container Options](struct.UploadToContainerOptions.html).
#[allow(missing_docs)]
pub trait UploadToContainerQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 2]>, Error>;
}

impl<'a, T: AsRef<str>> UploadToContainerQueryParams<&'a str, T> for UploadToContainerOptions<T> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, T); 2]>, Error> {
        Ok(ArrayVec::from([
            ("path", self.path),
            ("noOverwriteDirNonDir", self.no_overwrite_dir_non_dir),
        ]))
    }
}

/// Parameters used in the [Download From Container
/// API](../struct.Docker.html#method.download_from_container)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::DownloadFromContainerOptions;
///
/// DownloadFromContainerOptions{
///     path: "/opt",
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct DownloadFromContainerOptions<T>
where
    T: AsRef<str>,
{
    /// Resource in the container’s filesystem to archive.
    pub path: T,
}

/// Trait providing implementations for [Download From Container Options](struct.DownloadFromContainerOptions.html).
#[allow(missing_docs)]
pub trait DownloadFromContainerQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a, T: AsRef<str>> DownloadFromContainerQueryParams<&'a str, T>
    for DownloadFromContainerOptions<T>
{
    fn into_array(self) -> Result<ArrayVec<[(&'a str, T); 1]>, Error> {
        Ok(ArrayVec::from([("path", self.path)]))
    }
}
impl Docker {
    /// ---
    ///
    /// # List Containers
    ///
    /// Returns a list of containers.
    ///
    /// # Arguments
    ///
    ///  - Optional [ListContainersOptions](container/struct.ListContainersOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [APIContainers](container/struct.APIContainers.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::{ListContainersOptions};
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("health", vec!("unhealthy"));
    ///
    /// let options = Some(ListContainersOptions{
    ///     all: true,
    ///     filters: filters,
    ///     ..Default::default()
    /// });
    ///
    /// docker.list_containers(options);
    /// ```
    pub async fn list_containers<T, K>(
        &self,
        options: Option<T>,
    ) -> Result<Vec<APIContainers>, Error>
    where
        T: ListContainersQueryParams<K, String>,
        K: AsRef<str>,
    {
        let url = "/containers/json";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Create Container
    ///
    /// Prepares a container for a subsequent start operation.
    ///
    /// # Arguments
    ///
    ///  - Optional [Create Container Options](container/struct.CreateContainerOptions.html) struct.
    ///  - Container [Config](container/struct.Config.html) struct.
    ///
    /// # Returns
    ///
    ///  - [Create Container Results](container/struct.CreateContainerResults.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::{CreateContainerOptions, Config};
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(CreateContainerOptions{
    ///     name: "my-new-container",
    /// });
    ///
    /// let config = Config {
    ///     image: Some("hello-world"),
    ///     cmd: Some(vec!["/hello"]),
    ///     ..Default::default()
    /// };
    ///
    /// docker.create_container(options, config);
    /// ```
    pub async fn create_container<T, K, V, Z>(
        &self,
        options: Option<T>,
        config: Config<Z>,
    ) -> Result<CreateContainerResults, Error>
    where
        T: CreateContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
        Z: AsRef<str> + Eq + Hash + Serialize,
    {
        let url = "/containers/create";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Start Container
    ///
    /// Starts a container, after preparing it with the [Create Container
    /// API](struct.Docker.html#method.create_container).
    ///
    /// # Arguments
    ///
    ///  - Container name as a string slice.
    ///  - Optional [Start Container Options](container/struct.StartContainerOptions.html) struct.
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
    /// use bollard::container::StartContainerOptions;
    ///
    /// docker.start_container("hello-world", None::<StartContainerOptions<String>>);
    /// ```
    pub async fn start_container<T, K, V>(
        &self,
        container_name: &str,
        options: Option<T>,
    ) -> Result<(), Error>
    where
        T: StartContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/containers/{}/start", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Stop Container
    ///
    /// Stops a container.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - Optional [Stop Container Options](container/struct.StopContainerOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// use bollard::container::StopContainerOptions;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// let options = Some(StopContainerOptions{
    ///     t: 30,
    /// });
    ///
    /// docker.stop_container("hello-world", options);
    /// ```
    pub async fn stop_container<T, K>(
        &self,
        container_name: &str,
        options: Option<T>,
    ) -> Result<(), Error>
    where
        T: StopContainerQueryParams<K>,
        K: AsRef<str>,
    {
        let url = format!("/containers/{}/stop", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Remove Container
    ///
    /// Remove a container.
    ///
    /// # Arguments
    ///
    /// - Container name as a string slice.
    /// - Optional [Remove Container Options](container/struct.RemoveContainerOptions.html) struct.
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
    /// use bollard::container::RemoveContainerOptions;
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(RemoveContainerOptions{
    ///     force: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.remove_container("hello-world", options);
    /// ```
    pub async fn remove_container<T, K, V>(
        &self,
        container_name: &str,
        options: Option<T>,
    ) -> Result<(), Error>
    where
        T: RemoveContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/containers/{}", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Wait Container
    ///
    /// Wait for a container to stop. This is a non-blocking operation, the resulting stream will
    /// end when the container stops.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - Optional [Wait Container Options](container/struct.WaitContainerOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - [Wait Container Results](container/struct.WaitContainerResults.html), wrapped in a
    ///  Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::container::WaitContainerOptions;
    ///
    /// let options = Some(WaitContainerOptions{
    ///     condition: "not-running",
    /// });
    ///
    /// docker.wait_container("hello-world", options);
    /// ```
    pub fn wait_container<T, K, V>(
        &self,
        container_name: &str,
        options: Option<T>,
    ) -> impl Stream<Item = Result<WaitContainerResults, Error>>
    where
        T: WaitContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/containers/{}/wait", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_stream(req)
    }

    /// ---
    ///
    /// # Restart Container
    ///
    /// Restart a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - Optional [Restart Container Options](container/struct.RestartContainerOptions.html) struct.
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
    /// use bollard::container::RestartContainerOptions;
    ///
    /// let options = Some(RestartContainerOptions{
    ///     t: 30,
    /// });
    ///
    /// docker.restart_container("postgres", options);
    /// ```
    pub async fn restart_container<T, K>(
        &self,
        container_name: &str,
        options: Option<T>,
    ) -> Result<(), Error>
    where
        T: RestartContainerQueryParams<K>,
        K: AsRef<str>,
    {
        let url = format!("/containers/{}/restart", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Inspect Container
    ///
    /// Inspect a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as a string slice.
    ///  - Optional [Inspect Container Options](container/struct.InspectContainerOptions.struct) struct.
    ///
    /// # Returns
    ///
    ///  - [Container](container/struct.Container.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::InspectContainerOptions;
    ///
    /// let options = Some(InspectContainerOptions{
    ///     size: false,
    /// });
    ///
    /// docker.inspect_container("hello-world", options);
    /// ```
    pub async fn inspect_container<T, K, V>(
        &self,
        container_name: &str,
        options: Option<T>,
    ) -> Result<Container, Error>
    where
        T: InspectContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/containers/{}/json", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Top Processes
    ///
    /// List processes running inside a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - Optional [Top Options](container/struct.TopOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - [TopResult](container/struct.TopResult.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::TopOptions;
    ///
    /// let options = Some(TopOptions{
    ///     ps_args: "aux",
    /// });
    ///
    /// docker.top_processes("fussybeaver/uhttpd", options);
    /// ```
    pub async fn top_processes<T, K, V>(
        &self,
        container_name: &str,
        options: Option<T>,
    ) -> Result<TopResult, Error>
    where
        T: TopQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/containers/{}/top", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Logs
    ///
    /// Get container logs.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - Optional [Logs Options](container/struct.LogsOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - [Log Output](container/enum.LogOutput.html) enum, wrapped in a
    ///  Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::container::LogsOptions;
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(LogsOptions{
    ///     stdout: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.logs("hello-world", options);
    /// ```
    pub fn logs<T, K>(
        &self,
        container_name: &str,
        options: Option<T>,
    ) -> impl Stream<Item = Result<LogOutput, Error>>
    where
        T: LogsQueryParams<K>,
        K: AsRef<str>,
    {
        let url = format!("/containers/{}/logs", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_stream_string(req)
    }

    /// ---
    ///
    /// # Container Changes
    ///
    /// Get changes on a container's filesystem.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///
    /// # Returns
    ///
    ///  - An Option of Vector of [Change](container/struct.Change.html) structs, wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.container_changes("hello-world");
    /// ```
    pub async fn container_changes(
        &self,
        container_name: &str,
    ) -> Result<Option<Vec<Change>>, Error> {
        let url = format!("/containers/{}/changes", container_name);

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
    /// # Stats
    ///
    /// Get container stats based on resource usage.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - Optional [Stats Options](container/struct.StatsOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - [Stats](container/struct.Stats.html) struct, wrapped in a
    ///  Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::container::StatsOptions;
    ///
    /// let options = Some(StatsOptions{
    ///     stream: false,
    /// });
    ///
    /// docker.stats("hello-world", options);
    /// ```
    pub fn stats<T, K, V>(
        &self,
        container_name: &str,
        options: Option<T>,
    ) -> impl Stream<Item = Result<Stats, Error>>
    where
        T: StatsQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/containers/{}/stats", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_stream(req)
    }

    /// ---
    ///
    /// # Kill Container
    ///
    /// Kill a container.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - Optional [Kill Container Options](container/struct.KillContainerOptions.html) struct.
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
    /// use bollard::container::KillContainerOptions;
    ///
    /// let options = Some(KillContainerOptions{
    ///     signal: "SIGINT",
    /// });
    ///
    /// docker.kill_container("postgres", options);
    /// ```
    pub async fn kill_container<T, K, V>(
        &self,
        container_name: &str,
        options: Option<T>,
    ) -> Result<(), Error>
    where
        T: KillContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/containers/{}/kill", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Update Container
    ///
    /// Update a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - [Update Container Options](container/struct.UpdateContainerOptions.html) struct.
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
    /// use bollard::container::UpdateContainerOptions;
    /// use std::default::Default;
    ///
    /// let config = UpdateContainerOptions {
    ///     memory: Some(314572800),
    ///     memory_swap: Some(314572800),
    ///     ..Default::default()
    /// };
    ///
    /// docker.update_container("postgres", config);
    /// ```
    pub async fn update_container(
        &self,
        container_name: &str,
        config: UpdateContainerOptions,
    ) -> Result<(), Error> {
        let url = format!("/containers/{}/update", container_name);

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::POST),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Rename Container
    ///
    /// Rename a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - [Rename Container Options](container/struct.RenameContainerOptions.html) struct
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
    /// use bollard::container::RenameContainerOptions;
    ///
    /// let required = RenameContainerOptions {
    ///     name: "my_new_container_name"
    /// };
    ///
    /// docker.rename_container("hello-world", required);
    /// ```
    pub async fn rename_container<T, K, V>(
        &self,
        container_name: &str,
        options: T,
    ) -> Result<(), Error>
    where
        T: RenameContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/containers/{}/rename", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Docker::transpose_option(Some(options.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Pause Container
    ///
    /// Use the cgroups freezer to suspend all processes in a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as a string slice.
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
    /// docker.pause_container("postgres");
    /// ```
    pub async fn pause_container(&self, container_name: &str) -> Result<(), Error> {
        let url = format!("/containers/{}/pause", container_name);

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::POST),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Unpause Container
    ///
    /// Resume a container which has been paused.
    ///
    /// # Arguments
    ///
    ///  - Container name as a string slice.
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
    /// docker.unpause_container("postgres");
    /// ```
    pub async fn unpause_container(&self, container_name: &str) -> Result<(), Error> {
        let url = format!("/containers/{}/unpause", container_name);

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::POST),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Prune Containers
    ///
    /// Delete stopped containers.
    ///
    /// # Arguments
    ///
    ///  - Optional [Prune Containers Options](container/struct.PruneContainersOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - [PruneContainersResults](container/struct.PruneContainersResults.html) struct, wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::PruneContainersOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("until", vec!("10m"));
    ///
    /// let options = Some(PruneContainersOptions{
    ///     filters: filters
    /// });
    ///
    /// docker.prune_containers(options);
    /// ```
    pub async fn prune_containers<T, K>(
        &self,
        options: Option<T>,
    ) -> Result<PruneContainersResults, Error>
    where
        T: PruneContainersQueryParams<K>,
        K: AsRef<str> + Eq + Hash,
    {
        let url = "/containers/prune";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Upload To Container
    ///
    /// Upload a tar archive to be extracted to a path in the filesystem of container id.
    ///
    /// # Arguments
    ///
    ///  - Optional [Upload To Container Options](container/struct.UploadToContainerOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::UploadToContainerOptions;
    ///
    /// use std::default::Default;
    /// use std::fs::File;
    /// use std::io::Read;
    ///
    /// let options = Some(UploadToContainerOptions{
    ///     path: "/opt",
    ///     ..Default::default()
    /// });
    ///
    /// let mut file = File::open("tarball.tar.gz").unwrap();
    /// let mut contents = Vec::new();
    /// file.read_to_end(&mut contents).unwrap();
    ///
    /// docker.upload_to_container("my-container", options, contents.into());
    /// ```
    pub async fn upload_to_container<T, K, V>(
        &self,
        container_name: &str,
        options: Option<T>,
        tar: Body,
    ) -> Result<(), Error>
    where
        T: UploadToContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/containers/{}/archive", container_name);

        let req = self.build_request(
            &url,
            Builder::new()
                .method(Method::PUT)
                .header(CONTENT_TYPE, "application/x-tar"),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(tar),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Download From Container
    ///
    /// Get a tar archive of a resource in the filesystem of container id.
    ///
    /// # Arguments
    ///
    ///  - [Download From Container Options](container/struct.DownloadFromContainerOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - Tar archive compressed with one of the following algorithms: identity (no compression),
    ///    gzip, bzip2, xz. [Hyper Body](https://hyper.rs/hyper/master/hyper/struct.Body.html).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::DownloadFromContainerOptions;
    ///
    /// let options = Some(DownloadFromContainerOptions{
    ///     path: "/opt",
    /// });
    ///
    /// docker.download_from_container("my-container", options);
    /// ```
    pub fn download_from_container<T, K, V>(
        &self,
        container_name: &str,
        options: Option<T>,
    ) -> impl Stream<Item = Result<Bytes, Error>>
    where
        T: DownloadFromContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/containers/{}/archive", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_body(req)
    }
}
