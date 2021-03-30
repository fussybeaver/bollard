//! Container API: run docker containers and manage their lifecycle

use chrono::{DateTime, Utc};
use futures_core::Stream;
use http::header::CONTENT_TYPE;
use http::request::Builder;
use hyper::{body::Bytes, Body, Method};
use serde::Serialize;

use std::cmp::Eq;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;

use super::Docker;
use crate::errors::Error;

use crate::models::*;

/// Parameters used in the [List Container API](Docker::list_containers())
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
/// filters.insert("health", vec!["unhealthy"]);
///
/// ListContainersOptions{
///     all: true,
///     filters,
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct ListContainersOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
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
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

/// Parameters used in the [Create Container API](Docker::create_container())
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct CreateContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Assign the specified name to the container.
    pub name: T,
}

/// This container's networking configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct NetworkingConfig<T: Into<String> + Hash + Eq> {
    pub endpoints_config: HashMap<T, EndpointSettings>,
}

/// Container to create.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Config<T>
where
    T: Into<String> + Eq + Hash,
{
    /// The hostname to use for the container, as a valid RFC 1123 hostname.
    #[serde(rename = "Hostname")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<T>,

    /// The domain name to use for the container.
    #[serde(rename = "Domainname")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domainname: Option<T>,

    /// The user that commands are run as inside the container.
    #[serde(rename = "User")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<T>,

    /// Whether to attach to `stdin`.
    #[serde(rename = "AttachStdin")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attach_stdin: Option<bool>,

    /// Whether to attach to `stdout`.
    #[serde(rename = "AttachStdout")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attach_stdout: Option<bool>,

    /// Whether to attach to `stderr`.
    #[serde(rename = "AttachStderr")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attach_stderr: Option<bool>,

    /// An object mapping ports to an empty object in the form:  `{\"<port>/<tcp|udp|sctp>\": {}}`
    #[serde(rename = "ExposedPorts")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposed_ports: Option<HashMap<T, HashMap<(), ()>>>,

    /// Attach standard streams to a TTY, including `stdin` if it is not closed.
    #[serde(rename = "Tty")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tty: Option<bool>,

    /// Open `stdin`
    #[serde(rename = "OpenStdin")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_stdin: Option<bool>,

    /// Close `stdin` after one attached client disconnects
    #[serde(rename = "StdinOnce")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdin_once: Option<bool>,

    /// A list of environment variables to set inside the container in the form `[\"VAR=value\", ...]`. A variable without `=` is removed from the environment, rather than to have an empty value.
    #[serde(rename = "Env")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<T>>,

    /// Command to run specified as a string or an array of strings.
    #[serde(rename = "Cmd")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<Vec<T>>,

    /// A TEST to perform TO Check that the container is healthy.
    #[serde(rename = "Healthcheck")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<HealthConfig>,

    /// Command is already escaped (Windows only)
    #[serde(rename = "ArgsEscaped")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args_escaped: Option<bool>,

    /// The name of the image to use when creating the container
    #[serde(rename = "Image")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<T>,

    /// An object mapping mount point paths inside the container to empty objects.
    #[serde(rename = "Volumes")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volumes: Option<HashMap<T, HashMap<(), ()>>>,

    /// The working directory for commands to run in.
    #[serde(rename = "WorkingDir")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<T>,

    /// The entry point for the container as a string or an array of strings.  If the array consists of exactly one empty string (`[\"\"]`) then the entry point is reset to system default (i.e., the entry point used by docker when there is no `ENTRYPOINT` instruction in the `Dockerfile`).
    #[serde(rename = "Entrypoint")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<Vec<T>>,

    /// Disable networking for the container.
    #[serde(rename = "NetworkDisabled")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_disabled: Option<bool>,

    /// MAC address of the container.
    #[serde(rename = "MacAddress")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<T>,

    /// `ONBUILD` metadata that were defined in the image's `Dockerfile`.
    #[serde(rename = "OnBuild")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_build: Option<Vec<T>>,

    /// User-defined key/value metadata.
    #[serde(rename = "Labels")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<T, T>>,

    /// Signal to stop a container as a string or unsigned integer.
    #[serde(rename = "StopSignal")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_signal: Option<T>,

    /// Timeout to stop a container in seconds.
    #[serde(rename = "StopTimeout")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_timeout: Option<i64>,

    /// Shell for when `RUN`, `CMD`, and `ENTRYPOINT` uses a shell.
    #[serde(rename = "Shell")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<Vec<T>>,

    /// Container configuration that depends on the host we are running on.
    /// Shell for when `RUN`, `CMD`, and `ENTRYPOINT` uses a shell.
    #[serde(rename = "HostConfig")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_config: Option<HostConfig>,

    /// This container's networking configuration.
    #[serde(rename = "NetworkingConfig")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub networking_config: Option<NetworkingConfig<T>>,
}

impl From<ContainerConfig> for Config<String> {
    fn from(container: ContainerConfig) -> Self {
        Config {
            hostname: container.hostname,
            domainname: container.domainname,
            user: container.user,
            attach_stdin: container.attach_stdin,
            attach_stdout: container.attach_stdout,
            attach_stderr: container.attach_stderr,
            exposed_ports: container.exposed_ports,
            tty: container.tty,
            open_stdin: container.open_stdin,
            stdin_once: container.stdin_once,
            env: container.env,
            cmd: container.cmd,
            healthcheck: container.healthcheck,
            args_escaped: container.args_escaped,
            image: container.image,
            volumes: container.volumes,
            working_dir: container.working_dir,
            entrypoint: container.entrypoint,
            network_disabled: container.network_disabled,
            mac_address: container.mac_address,
            on_build: container.on_build,
            labels: container.labels,
            stop_signal: container.stop_signal,
            stop_timeout: container.stop_timeout,
            shell: container.shell,
            host_config: None,
            networking_config: None,
        }
    }
}

/// Parameters used in the [Stop Container API](Docker::stop_container())
///
/// ## Examples
///
/// use bollard::container::StopContainerOptions;
///
/// StopContainerOptions{
///     t: 30,
/// };
#[derive(Debug, Copy, Clone, Default, Serialize)]
pub struct StopContainerOptions {
    /// Number of seconds to wait before killing the container
    pub t: i64,
}

/// Parameters used in the [Start Container API](Docker::start_container())
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
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Override the key sequence for detaching a container. Format is a single character `[a-Z]` or
    /// `ctrl-<value>` where `<value>` is one of: `a-z`, `@`, `^`, `[`, `,` or `_`.
    pub detach_keys: T,
}

/// Parameters used in the [Remove Container API](Docker::remove_container())
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
#[derive(Debug, Copy, Clone, Default, Serialize)]
pub struct RemoveContainerOptions {
    /// Remove the volumes associated with the container.
    pub v: bool,
    /// If the container is running, kill it before removing it.
    pub force: bool,
    /// Remove the specified link associated with the container.
    pub link: bool,
}

/// Parameters used in the [Wait Container API](Docker::wait_container())
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct WaitContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Wait until a container state reaches the given condition, either 'not-running' (default),
    /// 'next-exit', or 'removed'.
    pub condition: T,
}

/// Parameters used in the [Restart Container API](Docker::restart_container())
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
#[derive(Debug, Copy, Clone, Default, Serialize)]
pub struct RestartContainerOptions {
    /// Number of seconds to wait before killing the container.
    pub t: isize,
}

/// Parameters used in the [Inspect Container API](Docker::inspect_container())
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
#[derive(Debug, Copy, Clone, Default, Serialize)]
pub struct InspectContainerOptions {
    /// Return the size of container as fields `SizeRw` and `SizeRootFs`
    pub size: bool,
}

/// Parameters used in the [Top Processes API](Docker::top_processes())
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
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopOptions<T>
where
    T: Into<String> + Serialize,
{
    /// The arguments to pass to `ps`. For example, `aux`
    pub ps_args: T,
}

/// Parameters used in the [Logs API](Docker::logs())
///
/// ## Examples
///
/// ```rust
/// use bollard::container::LogsOptions;
///
/// use std::default::Default;
///
/// LogsOptions::<String>{
///     stdout: true,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, Serialize)]
pub struct LogsOptions<T>
where
    T: Into<String> + Serialize,
{
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
    pub tail: T,
}

/// Result type for the [Logs API](Docker::logs())
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub enum LogOutput {
    StdErr { message: Bytes },
    StdOut { message: Bytes },
    StdIn { message: Bytes },
    Console { message: Bytes },
}

impl fmt::Display for LogOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match &self {
            LogOutput::StdErr { message } => message,
            LogOutput::StdOut { message } => message,
            LogOutput::StdIn { message } => message,
            LogOutput::Console { message } => message,
        };
        write!(f, "{}", String::from_utf8_lossy(&message))
    }
}

impl LogOutput {
    /// Get the raw bytes of the output
    pub fn into_bytes(self) -> Bytes {
        match self {
            LogOutput::StdErr { message } => message,
            LogOutput::StdOut { message } => message,
            LogOutput::StdIn { message } => message,
            LogOutput::Console { message } => message,
        }
    }
}

/// Parameters used in the [Stats API](super::Docker::stats())
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
#[derive(Debug, Copy, Clone, Default, Serialize)]
pub struct StatsOptions {
    /// Stream the output. If false, the stats will be output once and then it will disconnect.
    pub stream: bool,
}

/// Granular memory statistics for the container.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
#[serde(untagged)]
pub enum MemoryStatsStats {
    V1(MemoryStatsStatsV1),
    V2(MemoryStatsStatsV2),
}

/// Granular memory statistics for the container, v1 cgroups.
///
/// Exposed in the docker library [here](https://github.com/moby/moby/blob/40d9e2aff130b42ba0f83d5238b9b53184c8ab3b/daemon/daemon_unix.go#L1436).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
#[serde(deny_unknown_fields)]
pub struct MemoryStatsStatsV1 {
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

/// Granular memory statistics for the container, v2 cgroups.
///
/// Exposed in the docker library [here](https://github.com/moby/moby/blob/40d9e2aff130b42ba0f83d5238b9b53184c8ab3b/daemon/daemon_unix.go#L1542).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
#[serde(deny_unknown_fields)]
pub struct MemoryStatsStatsV2 {
    pub anon: u64,
    pub file: u64,
    pub kernel_stack: u64,
    pub slab: u64,
    pub sock: u64,
    pub shmem: u64,
    pub file_mapped: u64,
    pub file_dirty: u64,
    pub file_writeback: u64,
    pub anon_thp: u64,
    pub inactive_anon: u64,
    pub active_anon: u64,
    pub inactive_file: u64,
    pub active_file: u64,
    pub unevictable: u64,
    pub slab_reclaimable: u64,
    pub slab_unreclaimable: u64,
    pub pgfault: u64,
    pub pgmajfault: u64,
    pub workingset_refault: u64,
    pub workingset_activate: u64,
    pub workingset_nodereclaim: u64,
    pub pgrefill: u64,
    pub pgscan: u64,
    pub pgsteal: u64,
    pub pgactivate: u64,
    pub pgdeactivate: u64,
    pub pglazyfree: u64,
    pub pglazyfreed: u64,
    pub thp_fault_alloc: u64,
    pub thp_collapse_alloc: u64,
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

/// Parameters used in the [Kill Container API](Docker::kill_container())
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct KillContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Signal to send to the container as an integer or string (e.g. `SIGINT`)
    pub signal: T,
}

/// Configuration for the [Update Container API](Docker::update_container())
///
/// ## Examples
///
/// ```rust
/// use bollard::container::UpdateContainerOptions;
/// use std::default::Default;
///
/// UpdateContainerOptions::<String> {
///     memory: Some(314572800),
///     memory_swap: Some(314572800),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateContainerOptions<T>
where
    T: Into<String> + Eq + Hash,
{
    /// An integer value representing this container's relative CPU weight versus other containers.
    #[serde(rename = "CpuShares")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_shares: Option<isize>,

    /// Memory limit in bytes.
    #[serde(rename = "Memory")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<i64>,

    /// Path to `cgroups` under which the container's `cgroup` is created. If the path is not absolute, the path is considered to be relative to the `cgroups` path of the init process. Cgroups are created if they do not already exist.
    #[serde(rename = "CgroupParent")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgroup_parent: Option<T>,

    /// Block IO weight (relative weight).
    #[serde(rename = "BlkioWeight")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blkio_weight: Option<u16>,

    /// Block IO weight (relative device weight) in the form `[{\"Path\": \"device_path\", \"Weight\": weight}]`.
    #[serde(rename = "BlkioWeightDevice")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blkio_weight_device: Option<Vec<ResourcesBlkioWeightDevice>>,

    /// Limit read rate (bytes per second) from a device, in the form `[{\"Path\": \"device_path\", \"Rate\": rate}]`.
    #[serde(rename = "BlkioDeviceReadBps")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blkio_device_read_bps: Option<Vec<ThrottleDevice>>,

    /// Limit write rate (bytes per second) to a device, in the form `[{\"Path\": \"device_path\", \"Rate\": rate}]`.
    #[serde(rename = "BlkioDeviceWriteBps")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blkio_device_write_bps: Option<Vec<ThrottleDevice>>,

    /// Limit read rate (IO per second) from a device, in the form `[{\"Path\": \"device_path\", \"Rate\": rate}]`.
    #[serde(rename = "BlkioDeviceReadIOps")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blkio_device_read_i_ops: Option<Vec<ThrottleDevice>>,

    /// Limit write rate (IO per second) to a device, in the form `[{\"Path\": \"device_path\", \"Rate\": rate}]`.
    #[serde(rename = "BlkioDeviceWriteIOps")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blkio_device_write_i_ops: Option<Vec<ThrottleDevice>>,

    /// The length of a CPU period in microseconds.
    #[serde(rename = "CpuPeriod")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_period: Option<i64>,

    /// Microseconds of CPU time that the container can get in a CPU period.
    #[serde(rename = "CpuQuota")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_quota: Option<i64>,

    /// The length of a CPU real-time period in microseconds. Set to 0 to allocate no time allocated to real-time tasks.
    #[serde(rename = "CpuRealtimePeriod")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_realtime_period: Option<i64>,

    /// The length of a CPU real-time runtime in microseconds. Set to 0 to allocate no time allocated to real-time tasks.
    #[serde(rename = "CpuRealtimeRuntime")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_realtime_runtime: Option<i64>,

    /// CPUs in which to allow execution (e.g., `0-3`, `0,1`)
    #[serde(rename = "CpusetCpus")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpuset_cpus: Option<T>,

    /// Memory nodes (MEMs) in which to allow execution (0-3, 0,1). Only effective on NUMA systems.
    #[serde(rename = "CpusetMems")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpuset_mems: Option<T>,

    /// A list of devices to add to the container.
    #[serde(rename = "Devices")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<DeviceMapping>>,

    /// a list of cgroup rules to apply to the container
    #[serde(rename = "DeviceCgroupRules")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_cgroup_rules: Option<Vec<T>>,

    /// a list of requests for devices to be sent to device drivers
    #[serde(rename = "DeviceRequests")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_requests: Option<Vec<DeviceRequest>>,

    /// Kernel memory limit in bytes.
    #[serde(rename = "KernelMemory")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_memory: Option<i64>,

    /// Hard limit for kernel TCP buffer memory (in bytes).
    #[serde(rename = "KernelMemoryTCP")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_memory_tcp: Option<i64>,

    /// Memory soft limit in bytes.
    #[serde(rename = "MemoryReservation")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_reservation: Option<i64>,

    /// Total memory limit (memory + swap). Set as `-1` to enable unlimited swap.
    #[serde(rename = "MemorySwap")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_swap: Option<i64>,

    /// Tune a container's memory swappiness behavior. Accepts an integer between 0 and 100.
    #[serde(rename = "MemorySwappiness")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_swappiness: Option<i64>,

    /// CPU quota in units of 10<sup>-9</sup> CPUs.
    #[serde(rename = "NanoCPUs")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nano_cp_us: Option<i64>,

    /// Disable OOM Killer for the container.
    #[serde(rename = "OomKillDisable")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oom_kill_disable: Option<bool>,

    /// Run an init inside the container that forwards signals and reaps processes. This field is omitted if empty, and the default (as configured on the daemon) is used.
    #[serde(rename = "Init")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init: Option<bool>,

    /// Tune a container's PIDs limit. Set `0` or `-1` for unlimited, or `null` to not change.
    #[serde(rename = "PidsLimit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids_limit: Option<i64>,

    /// A list of resource limits to set in the container. For example: `{\"Name\": \"nofile\", \"Soft\": 1024, \"Hard\": 2048}`\"
    #[serde(rename = "Ulimits")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ulimits: Option<Vec<ResourcesUlimits>>,

    /// The number of usable CPUs (Windows only).  On Windows Server containers, the processor resource controls are mutually exclusive. The order of precedence is `CPUCount` first, then `CPUShares`, and `CPUPercent` last.
    #[serde(rename = "CpuCount")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_count: Option<i64>,

    /// The usable percentage of the available CPUs (Windows only).  On Windows Server containers, the processor resource controls are mutually exclusive. The order of precedence is `CPUCount` first, then `CPUShares`, and `CPUPercent` last.
    #[serde(rename = "CpuPercent")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<i64>,

    /// Maximum IOps for the container system drive (Windows only)
    #[serde(rename = "IOMaximumIOps")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_maximum_i_ops: Option<i64>,

    /// Maximum IO in bytes per second for the container system drive (Windows only)
    #[serde(rename = "IOMaximumBandwidth")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_maximum_bandwidth: Option<i64>,

    /// The behavior to apply when the container exits. The default is not to restart.
    ///
    /// An ever increasing delay (double the previous delay, starting at 100ms) is added before
    /// each restart to prevent flooding the server.
    pub restart_policy: Option<RestartPolicy>,
}

/// Parameters used in the [Rename Container API](Docker::rename_container())
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct RenameContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// New name for the container.
    pub name: T,
}

/// Parameters used in the [Prune Containers API](Docker::prune_containers())
///
/// ## Examples
///
/// ```rust
/// use bollard::container::PruneContainersOptions;
///
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("until", vec!["10m"]);
///
/// PruneContainersOptions{
///     filters
/// };
/// ```
#[derive(Debug, Clone, Default, Serialize)]
pub struct PruneContainersOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
{
    /// Filters to process on the prune list, encoded as JSON.
    ///
    /// Available filters:
    ///  - `until=<timestamp>` Prune containers created before this timestamp. The `<timestamp>` can be Unix timestamps, date formatted timestamps, or Go duration strings (e.g. `10m`, `1h30m`) computed relative to the daemon machine's time.
    ///  - label (`label=<key>`, `label=<key>=<value>`, `label!=<key>`, or `label!=<key>=<value>`) Prune containers with (or without, in case `label!=...` is used) the specified labels.
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

/// Parameters used in the [Upload To Container
/// API](Docker::upload_to_container)
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
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadToContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Path to a directory in the container to extract the archive’s contents into.
    pub path: T,
    /// If “1”, “true”, or “True” then it will be an error if unpacking the given content would
    /// cause an existing directory to be replaced with a non-directory and vice versa.
    pub no_overwrite_dir_non_dir: T,
}

/// Parameters used in the [Download From Container
/// API](Docker::download_from_container())
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct DownloadFromContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Resource in the container’s filesystem to archive.
    pub path: T,
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
    ///  - Optional [ListContainersOptions](ListContainersOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [ContainerSummaryInner](ContainerSummaryInner), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::ListContainersOptions;
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("health", vec!["unhealthy"]);
    ///
    /// let options = Some(ListContainersOptions{
    ///     all: true,
    ///     filters,
    ///     ..Default::default()
    /// });
    ///
    /// docker.list_containers(options);
    /// ```
    pub async fn list_containers<'de, T>(
        &self,
        options: Option<ListContainersOptions<T>>,
    ) -> Result<Vec<ContainerSummaryInner>, Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/containers/json";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            options,
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
    ///  - Optional [Create Container Options](CreateContainerOptions) struct.
    ///  - Container [Config](Config) struct.
    ///
    /// # Returns
    ///
    ///  - [ContainerCreateResponse](ContainerCreateResponse), wrapped in a Future.
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
    pub async fn create_container<T, Z>(
        &self,
        options: Option<CreateContainerOptions<T>>,
        config: Config<Z>,
    ) -> Result<ContainerCreateResponse, Error>
    where
        T: Into<String> + Serialize,
        Z: Into<String> + Hash + Eq + Serialize,
    {
        let url = "/containers/create";
        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            options,
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
    ///  - Optional [Start Container Options](StartContainerOptions) struct.
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
    pub async fn start_container<T>(
        &self,
        container_name: &str,
        options: Option<StartContainerOptions<T>>,
    ) -> Result<(), Error>
    where
        T: Into<String> + Serialize,
    {
        let url = format!("/containers/{}/start", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options,
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
    /// - Optional [Stop Container Options](StopContainerOptions) struct.
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
    pub async fn stop_container(
        &self,
        container_name: &str,
        options: Option<StopContainerOptions>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{}/stop", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options,
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
    /// - Optional [Remove Container Options](RemoveContainerOptions) struct.
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
    pub async fn remove_container(
        &self,
        container_name: &str,
        options: Option<RemoveContainerOptions>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{}", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            options,
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
    /// - Optional [Wait Container Options](WaitContainerOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [ContainerWaitResponse](ContainerWaitResponse), wrapped in a
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
    pub fn wait_container<T>(
        &self,
        container_name: &str,
        options: Option<WaitContainerOptions<T>>,
    ) -> impl Stream<Item = Result<ContainerWaitResponse, Error>>
    where
        T: Into<String> + Serialize,
    {
        let url = format!("/containers/{}/wait", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options,
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
    ///  - Optional [Restart Container Options](RestartContainerOptions) struct.
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
    pub async fn restart_container(
        &self,
        container_name: &str,
        options: Option<RestartContainerOptions>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{}/restart", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options,
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
    ///  - Optional [Inspect Container Options](InspectContainerOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [ContainerInspectResponse](ContainerInspectResponse), wrapped in a Future.
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
    pub async fn inspect_container(
        &self,
        container_name: &str,
        options: Option<InspectContainerOptions>,
    ) -> Result<ContainerInspectResponse, Error> {
        let url = format!("/containers/{}/json", container_name);

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
    /// # Top Processes
    ///
    /// List processes running inside a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - Optional [Top Options](TopOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [ContainerTopResponse](ContainerTopResponse), wrapped in a Future.
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
    pub async fn top_processes<T>(
        &self,
        container_name: &str,
        options: Option<TopOptions<T>>,
    ) -> Result<ContainerTopResponse, Error>
    where
        T: Into<String> + Serialize,
    {
        let url = format!("/containers/{}/top", container_name);

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
    /// # Logs
    ///
    /// Get container logs.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - Optional [Logs Options](LogsOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [Log Output](LogOutput) enum, wrapped in a
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
    /// let options = Some(LogsOptions::<String>{
    ///     stdout: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.logs("hello-world", options);
    /// ```
    pub fn logs<T>(
        &self,
        container_name: &str,
        options: Option<LogsOptions<T>>,
    ) -> impl Stream<Item = Result<LogOutput, Error>>
    where
        T: Into<String> + Serialize,
    {
        let url = format!("/containers/{}/logs", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options,
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
    ///  - An Option of Vector of [Container Change Response Item](ContainerChangeResponseItem) structs, wrapped in a
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
    ) -> Result<Option<Vec<ContainerChangeResponseItem>>, Error> {
        let url = format!("/containers/{}/changes", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            None::<String>,
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
    /// - Optional [Stats Options](StatsOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [Stats](Stats) struct, wrapped in a
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
    pub fn stats(
        &self,
        container_name: &str,
        options: Option<StatsOptions>,
    ) -> impl Stream<Item = Result<Stats, Error>> {
        let url = format!("/containers/{}/stats", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options,
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
    /// - Optional [Kill Container Options](KillContainerOptions) struct.
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
    pub async fn kill_container<T>(
        &self,
        container_name: &str,
        options: Option<KillContainerOptions<T>>,
    ) -> Result<(), Error>
    where
        T: Into<String> + Serialize,
    {
        let url = format!("/containers/{}/kill", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options,
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
    ///  - [Update Container Options](UpdateContainerOptions) struct.
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
    /// let config = UpdateContainerOptions::<String> {
    ///     memory: Some(314572800),
    ///     memory_swap: Some(314572800),
    ///     ..Default::default()
    /// };
    ///
    /// docker.update_container("postgres", config);
    /// ```
    pub async fn update_container<T>(
        &self,
        container_name: &str,
        config: UpdateContainerOptions<T>,
    ) -> Result<(), Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = format!("/containers/{}/update", container_name);

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
    /// # Rename Container
    ///
    /// Rename a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - [Rename Container Options](RenameContainerOptions) struct
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
    pub async fn rename_container<T>(
        &self,
        container_name: &str,
        options: RenameContainerOptions<T>,
    ) -> Result<(), Error>
    where
        T: Into<String> + Serialize,
    {
        let url = format!("/containers/{}/rename", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Some(options),
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

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
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

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
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
    ///  - Optional [Prune Containers Options](PruneContainersOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [Container Prune Response](ContainerPruneResponse) struct, wrapped in a Future.
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
    /// filters.insert("until", vec!["10m"]);
    ///
    /// let options = Some(PruneContainersOptions{
    ///     filters
    /// });
    ///
    /// docker.prune_containers(options);
    /// ```
    pub async fn prune_containers<T>(
        &self,
        options: Option<PruneContainersOptions<T>>,
    ) -> Result<ContainerPruneResponse, Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/containers/prune";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options,
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
    ///  - Optional [Upload To Container Options](UploadToContainerOptions) struct.
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
    pub async fn upload_to_container<T>(
        &self,
        container_name: &str,
        options: Option<UploadToContainerOptions<T>>,
        tar: Body,
    ) -> Result<(), Error>
    where
        T: Into<String> + Serialize,
    {
        let url = format!("/containers/{}/archive", container_name);

        let req = self.build_request(
            &url,
            Builder::new()
                .method(Method::PUT)
                .header(CONTENT_TYPE, "application/x-tar"),
            options,
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
    ///  - [Download From Container Options](DownloadFromContainerOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Tar archive compressed with one of the following algorithms: identity (no compression),
    ///    gzip, bzip2, xz. [Hyper Body](hyper::body::Body).
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
    pub fn download_from_container<T>(
        &self,
        container_name: &str,
        options: Option<DownloadFromContainerOptions<T>>,
    ) -> impl Stream<Item = Result<Bytes, Error>>
    where
        T: Into<String> + Serialize,
    {
        let url = format!("/containers/{}/archive", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options,
            Ok(Body::empty()),
        );

        self.process_into_body(req)
    }
}
