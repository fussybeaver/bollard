use chrono::serde::ts_seconds;
use chrono::{DateTime, Utc};
use failure::Error;
use futures::{stream, Stream};
use http::request::Builder;
use hyper::client::connect::Connect;
use hyper::rt::Future;
use hyper::Method;
use serde_json;

use std::collections::HashMap;
use std::fmt;

use super::{Docker, DockerChain};
use either::EitherStream;
use options::{EncodableQueryString, NoParams};

/// ## List Container Options
///
/// Parameters used in the [List Container API](../struct.Docker.html#method.list_container)
#[derive(Debug, Clone, Default)]
pub struct ListContainersOptions {
    pub all: bool,
    pub limit: Option<isize>,
    pub size: bool,
    pub filters: Option<String>,
}

impl EncodableQueryString for ListContainersOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![
            ("all", self.all.to_string()),
            (
                "limit",
                self.limit
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| String::new()),
            ),
            ("size", self.size.to_string()),
            ("filters", serde_json::to_string(&self.filters)?),
        ])
    }
}

/// ## Create Container Options
///
/// Parameters used in the [Create Container API](../struct.Docker.html#method.create_container)
#[derive(Debug, Clone, Default)]
pub struct CreateContainerOptions {
    pub name: String,
}

impl EncodableQueryString for CreateContainerOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("name", self.name)])
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct HostConfig {
    pub binds: Option<Vec<String>>,
    pub links: Option<Vec<String>>,
    pub memory: Option<u64>,
    pub memory_swap: Option<u64>,
    pub memory_reservation: Option<u64>,
    pub kernel_memory: Option<u64>,
    pub nano_cpus: Option<u64>,
    pub cpu_percent: Option<u64>,
    pub cpu_shares: Option<u64>,
    pub cpu_period: Option<u64>,
    pub cpu_realtime_period: Option<u64>,
    pub cpu_realtime_runtime: Option<u64>,
    pub cpu_quota: Option<u64>,
    pub cpuset_cpus: Option<String>,
    pub cpuset_mems: Option<String>,
    #[serde(rename = "MaximumIOPs")]
    pub maximum_iops: Option<u64>,
    #[serde(rename = "MaximumIOBPs")]
    pub maximum_iobps: Option<u64>,
    pub blkio_weight: Option<u64>,
    pub blkio_weight_device: Option<Vec<HashMap<String, String>>>,
    pub blkio_device_read_bps: Option<Vec<HashMap<String, String>>>,
    pub blkio_device_write_bps: Option<Vec<HashMap<String, String>>>,
    #[serde(rename = "BlkioDeviceReadIOps")]
    pub blkio_device_read_iops: Option<Vec<HashMap<String, String>>>,
    #[serde(rename = "BlkioDeviceWriteIOps")]
    pub blkio_device_write_iops: Option<Vec<HashMap<String, String>>>,
    pub memory_swappiness: Option<u64>,
    pub oom_kill_disable: Option<bool>,
    pub oom_score_adj: Option<isize>,
    pub pid_mode: Option<String>,
    pub pids_limit: Option<u64>,
    pub port_bindings: Option<HashMap<String, Vec<PortBinding>>>,
    pub publish_all_ports: Option<bool>,
    pub privileged: Option<bool>,
    pub readonly_rootfs: Option<bool>,
    pub dns: Option<Vec<String>>,
    pub dns_options: Option<Vec<String>>,
    pub dns_search: Option<Vec<String>>,
    pub volumes_from: Option<Vec<String>>,
    pub cap_add: Option<Vec<String>>,
    pub cap_drop: Option<Vec<String>>,
    pub group_add: Option<Vec<String>>,
    pub restart_policy: Option<RestartPolicy>,
    pub auto_remove: Option<bool>,
    pub network_mode: Option<String>,
    pub devices: Option<Vec<String>>,
    pub ulimits: Option<Vec<HashMap<String, String>>>,
    pub log_config: Option<LogConfig>,
    pub security_opt: Option<Vec<String>>,
    pub storage_opt: Option<HashMap<String, String>>,
    pub cgroup_parent: Option<String>,
    pub volume_driver: Option<String>,
    pub shm_size: Option<u64>,
    #[serde(rename = "ContainerIDFile")]
    pub container_id_file: Option<String>,
    pub extra_hosts: Option<Vec<String>>,
    pub ipc_mode: Option<String>,
    pub cgroup: Option<String>,
    #[serde(rename = "UTSMode")]
    pub uts_mode: Option<String>,
    pub userns_mode: Option<String>,
    pub runtime: Option<String>,
    pub console_size: Option<Vec<usize>>,
    pub isolation: Option<String>,
    pub device_cgroup_rules: Option<Vec<String>>,
    pub disk_quota: Option<u64>,
    pub cpu_count: Option<u64>,
    #[serde(rename = "IOMaximumIOps")]
    pub io_maximum_iops: Option<u64>,
    #[serde(rename = "IOMaximumBandwidth")]
    pub io_maximum_bandwidth: Option<u64>,
    pub masked_paths: Option<Vec<String>>,
    pub readonly_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct GraphDriver {
    pub name: String,
    pub data: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct PortBinding {
    #[serde(rename = "HostIP")]
    pub host_ip: String,
    pub host_port: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct RestartPolicy {
    pub name: Option<String>,
    pub maximum_retry_count: Option<isize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct LogConfig {
    #[serde(rename = "Type")]
    pub type_: Option<String>,
    pub config: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkingConfig {
    pub endpoints_config: HashMap<String, ContainerNetwork>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointIPAMConfig {
    #[serde(rename = "IPV4Address")]
    pub ipv4_address: String,
    #[serde(rename = "IPV6Address")]
    pub ipv6_address: String,
}

/*
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct EndpointConfig {
    #[serde(rename = "IPAMConfig")]
    pub ipam_config: Option<EndpointIPAMConfig>,
    pub links: Option<Vec<String>>,
    pub aliases: Option<Vec<String>>,
    #[serde(rename = "NetworkID")]
    pub network_id: Option<String>,
    #[serde(rename = "EndpointID")]
    pub endpoint_id: Option<String>,
    pub gateway: Option<String>,
    #[serde(rename = "IPAddress")]
    pub ip_address: Option<String>,
    pub ip_prefix_len: Option<u64>,
    #[serde(rename = "IPv6Gateway")]
    pub ipv6_gateway: Option<String>,
    #[serde(rename = "GlobalIPv6Address")]
    pub global_ipv6_address: Option<String>,
    #[serde(rename = "GlobalIPv6PrefixLen")]
    pub global_ipv6_prefix_len: Option<u64>,
    pub mac_address: Option<String>,
}
*/

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct ContainerNetwork {
    #[serde(rename = "IPAMConfig")]
    pub ipam_config: Option<EndpointIPAMConfig>,
    pub links: Option<Vec<String>>,
    pub aliases: Option<Vec<String>>,
    pub mac_address: String,
    #[serde(rename = "GlobalIPv6Address")]
    pub global_ipv6_address: String,
    #[serde(rename = "GlobalIPv6PrefixLen")]
    pub global_ipv6_prefix_len: usize,
    #[serde(rename = "IPv6Gateway")]
    pub ipv6_gateway: String,
    #[serde(rename = "IPAddress")]
    pub ip_address: String,
    #[serde(rename = "IPPrefixLen")]
    pub ip_prefix_len: u64,
    pub gateway: String,
    #[serde(rename = "EndpointID")]
    pub endpoint_id: String,
    #[serde(rename = "NetworkID")]
    pub network_id: String,
    pub driver_opts: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct NetworkSettings {
    pub networks: HashMap<String, ContainerNetwork>,
    #[serde(rename = "IPAddress")]
    pub ip_address: String,
    #[serde(rename = "IPPrefixLen")]
    pub ip_prefix_len: usize,
    pub mac_address: String,
    pub gateway: String,
    pub bridge: String,
    #[serde(rename = "EndpointID")]
    pub endpoint_id: String,
    pub sandbox_key: String,
    #[serde(rename = "GlobalIPv6Address")]
    pub global_ipv6_address: String,
    #[serde(rename = "GlobalIPv6PrefixLen")]
    pub global_ipv6_prefix_len: usize,
    #[serde(rename = "IPv6Gateway")]
    pub ipv6_gateway: String,
    #[serde(rename = "LinkLocalIPv6Address")]
    pub link_local_ipv6_address: String,
    #[serde(rename = "LinkLocalIPv6PrefixLen")]
    pub link_local_ipv6_prefix_len: usize,
    #[serde(rename = "SecondaryIPAddresses")]
    pub secondary_ip_addresses: Option<Vec<String>>,
    #[serde(rename = "SecondaryIPv6Addresses")]
    pub secondary_ipv6_addresses: Option<Vec<String>>,
    #[serde(rename = "SandboxID")]
    pub sandbox_id: String,
    pub hairpin_mode: bool,
    pub ports: HashMap<String, Option<Vec<PortBinding>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Mount {
    pub name: String,
    pub source: String,
    pub destination: String,
    pub driver: String,
    pub mode: String,
    #[serde(rename = "RW")]
    pub rw: bool,
    #[serde(rename = "Type")]
    pub type_: String,
    pub propagation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct State {
    pub status: String,
    pub running: bool,
    pub paused: bool,
    pub restarting: bool,
    #[serde(rename = "OOMKilled")]
    pub oomkilled: bool,
    pub dead: bool,
    pub pid: usize,
    pub exit_code: u16,
    pub error: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct APIPort {
    #[serde(rename = "IP")]
    pub ip: Option<String>,
    pub private_port: i64,
    pub public_port: Option<i64>,
    #[serde(rename = "Type")]
    pub type_: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct NetworkList {
    pub networks: HashMap<String, ContainerNetwork>,
}

/// ## API Containers
///
/// Result type for the [List Containers API](../struct.Docker.html#method.list_containers)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
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
    pub host_config: HostConfig,
}

/// ## Container
///
/// Result type for the [Inspect Container API](../struct.Docker.html#method.inspect_container)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Container {
    pub id: String,
    pub created: DateTime<Utc>,
    pub path: String,
    pub args: Vec<String>,
    pub config: Config,
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
    pub host_config: HostConfig,
    pub restart_count: usize,
    pub platform: String,
    pub mount_label: String,
    pub process_label: String,
    pub app_armor_profile: String,
    #[serde(rename = "ExecIDs")]
    pub exec_ids: Option<Vec<String>>,
    pub graph_driver: GraphDriver,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Config {
    pub hostname: Option<String>,
    pub domainname: Option<String>,
    pub user: Option<String>,
    pub attach_stdin: Option<bool>,
    pub attach_stdout: Option<bool>,
    pub attach_stderr: Option<bool>,
    pub args_escaped: Option<bool>,
    pub tty: Option<bool>,
    pub open_stdin: Option<bool>,
    pub stdin_once: Option<bool>,
    pub env: Option<Vec<String>>,
    pub cmd: Vec<String>,
    pub entrypoint: Option<Vec<String>>,
    pub image: Option<String>,
    pub labels: Option<HashMap<String, String>>,
    pub volumes: Option<HashMap<String, HashMap<(), ()>>>,
    pub working_dir: Option<String>,
    pub network_disabled: Option<bool>,
    pub on_build: Option<Vec<String>>,
    pub mac_address: Option<String>,
    pub exposed_ports: Option<HashMap<String, HashMap<(), ()>>>,
    pub stop_signal: Option<String>,
    pub stop_timeout: Option<usize>,
    pub host_config: Option<HostConfig>,
    pub networking_config: Option<NetworkingConfig>,
}

/// ## Create Container Results
///
/// Result type for the [Create Container API](../struct.Docker.html#method.create_container)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct CreateContainerResults {
    pub id: String,
    pub warnings: Option<Vec<String>>,
}

/// ## Stop Container Options
///
/// Parameters used in the [Stop Container API](../struct.Docker.html#method.stop_container)
#[derive(Debug, Clone, Default)]
pub struct StopContainerOptions {
    pub t: u64,
}

impl EncodableQueryString for StopContainerOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("t", self.t.to_string())])
    }
}

/// ## Start Container Options
///
/// Parameters used in the [Start Container API](../struct.Docker.html#method.start_container)
#[derive(Debug, Clone, Default)]
pub struct StartContainerOptions {
    pub detach_keys: String,
}

impl EncodableQueryString for StartContainerOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("detachKeys", self.detach_keys)])
    }
}

/// ## Remove Container Options
///
/// Parameters used in the [Remove Container API](../struct.Docker.html#method.remove_container)
#[derive(Debug, Clone, Default)]
pub struct RemoveContainerOptions {
    pub v: bool,
    pub force: bool,
    pub link: bool,
}

impl EncodableQueryString for RemoveContainerOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![
            ("v", self.v.to_string()),
            ("force", self.force.to_string()),
            ("link", self.link.to_string()),
        ])
    }
}

/// ## Wait Container Options
///
/// Parameters used in the [Wait Container API](../struct.Docker.html#method.wait_container)
#[derive(Debug, Clone, Default)]
pub struct WaitContainerOptions {
    pub condition: String,
}

impl EncodableQueryString for WaitContainerOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("condition", self.condition)])
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct WaitContainerResultsError {
    pub message: String,
}

/// ## Wait Container Results
///
/// Result type for the [Wait Container API](../struct.Docker.html#method.wait_container)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct WaitContainerResults {
    pub status_code: u16,
    pub error: Option<WaitContainerResultsError>,
}

/// ## Restart Container Options
///
/// Parameters used in the [Restart Container API](../struct.Docker.html#method.restart_container)
#[derive(Debug, Clone, Default)]
pub struct RestartContainerOptions {
    pub t: usize,
}

impl EncodableQueryString for RestartContainerOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("t", self.t.to_string())])
    }
}

/// ## Inspect Container Options
///
/// Parameters used in the [Inspect Container API](../struct.Docker.html#method.inspect_container)
#[derive(Debug, Clone, Default)]
pub struct InspectContainerOptions {
    pub size: bool,
}

impl EncodableQueryString for InspectContainerOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("size", self.size.to_string())])
    }
}

/// ## Top Options
///
/// Parameters used in the [Top Processes API](../struct.Docker.html#method.top_processes)
#[derive(Debug, Clone, Default)]
pub struct TopOptions {
    pub ps_args: String,
}

impl EncodableQueryString for TopOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("ps_args", self.ps_args.to_string())])
    }
}

/// ## Top Result
///
/// Result type for the [Top Processes API](../struct.Docker.html#method.top_processes)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct TopResult {
    pub titles: Vec<String>,
    pub processes: Vec<Vec<String>>,
}

/// ## Logs Options
///
/// Parameters used in the [Logs API](../struct.Docker.html#method.logs)
#[derive(Debug, Clone, Default)]
pub struct LogsOptions {
    pub follow: bool,
    pub stdout: bool,
    pub stderr: bool,
    pub since: u64,
    pub until: u64,
    pub timestamps: bool,
    pub tail: String,
}

impl EncodableQueryString for LogsOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![
            ("follow", self.follow.to_string()),
            ("stdout", self.stdout.to_string()),
            ("stderr", self.stderr.to_string()),
            ("since", self.since.to_string()),
            ("until", self.until.to_string()),
            ("timestamps", self.timestamps.to_string()),
            ("tail", self.tail),
        ])
    }
}

/// ## Log Output
///
/// Result type for the [Logs API](../struct.Docker.html#method.logs)
#[derive(Debug, Clone)]
pub enum LogOutput {
    StdErr { message: String },
    StdOut { message: String },
    StdIn { message: String },
}

impl fmt::Display for LogOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            LogOutput::StdErr { message } => write!(f, "{}", message),
            LogOutput::StdOut { message } => write!(f, "{}", message),
            LogOutput::StdIn { message } => write!(f, "{}", message),
        }
    }
}

/// ## Change
///
/// Result type for the [Container Changes API](../struct.Docker.html#method.container_changes)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Change {
    pub path: String,
    pub kind: usize,
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            0 => write!(f, "C {}", self.path),
            1 => write!(f, "A {}", self.path),
            2 => write!(f, "D {}", self.path),
            _ => unreachable!(),
        }
    }
}

/// ## Stats Options
///
/// Parameters used in the [Stats API](../struct.Docker.html#method.stats)
#[derive(Debug, Clone, Default)]
pub struct StatsOptions {
    pub stream: bool,
}

impl EncodableQueryString for StatsOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("stream", self.stream.to_string())])
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryStats {
    pub stats: Option<MemoryStatsStats>,
    pub max_usage: Option<u64>,
    pub usage: Option<u64>,
    pub failcnt: Option<u64>,
    pub limit: Option<u64>,
    pub commit: Option<u64>,
    pub commit_peak: Option<u64>,
    pub commitbytes: Option<u64>,
    //pub commitpeakbytes: Option<u64>,
    pub private_working_set: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PidsStats {
    pub current: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageStats {
    pub read_count_normalized: Option<u64>,
    pub read_size_bytes: Option<u64>,
    pub write_count_normalized: Option<u64>,
    pub write_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CPUUsage {
    pub percpu_usage: Option<Vec<u64>>,
    pub usage_in_usermode: u64,
    pub total_usage: u64,
    pub usage_in_kernelmode: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThrottlingData {
    pub periods: u64,
    pub throttled_periods: u64,
    pub throttled_time: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CPUStats {
    pub cpu_usage: CPUUsage,
    pub system_cpu_usage: Option<u64>,
    pub online_cpus: Option<u64>,
    pub throttling_data: ThrottlingData,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlkioStatsEntry {
    pub major: u64,
    pub minor: u64,
    pub op: String,
    pub value: u64,
}

/// ## Kill Container Options
///
/// Parameters used in the [Kill Container API](../struct.Docker.html#method.kill_container)
#[derive(Debug, Clone, Default)]
pub struct KillContainerOptions {
    pub signal: String,
}

impl EncodableQueryString for KillContainerOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("signal", self.signal)])
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct UpdateContainerOptionsBlkioWeight {
    pub path: String,
    pub weight: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct UpdateContainerOptionsBlkioDeviceRate {
    pub path: String,
    pub rate: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct UpdateContainerOptionsDevices {
    pub path_on_host: String,
    pub path_in_container: String,
    pub cgroup_permissions: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct UpdateContainerOptionsUlimits {
    pub name: String,
    pub soft: isize,
    pub hard: isize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct UpdateContainerOptionsRestartPolicy {
    pub name: String,
    pub maximum_retry_count: isize,
}

/// ## Update Container Options
///
/// Configuration for the [Update Container API](../struct.Docker.html#method.update_container)
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct UpdateContainerOptions {
    pub cpu_shares: Option<isize>,
    pub memory: Option<i64>,
    pub cgroup_parent: Option<String>,
    pub blkio_weight: Option<isize>,
    pub blkio_weight_device: Vec<UpdateContainerOptionsBlkioWeight>,
    pub blkio_device_read_bps: Vec<UpdateContainerOptionsBlkioDeviceRate>,
    #[serde(rename = "BlkioDeviceWriteIOps")]
    pub blkio_device_write_iops: Vec<UpdateContainerOptionsBlkioDeviceRate>,
    #[serde(rename = "BlkioDeviceReadIOps")]
    pub blkio_device_read_iops: Vec<UpdateContainerOptionsBlkioDeviceRate>,
    pub cpu_period: Option<i64>,
    pub cpu_quota: Option<i64>,
    pub cpu_realtime_period: Option<i64>,
    pub cpu_realtime_runtime: Option<i64>,
    pub cpuset_cpus: Option<String>,
    pub cpuset_mems: Option<String>,
    pub devices: Option<Vec<UpdateContainerOptionsDevices>>,
    pub device_cgroup_rules: Option<Vec<String>>,
    pub disk_quota: Option<i64>,
    pub kernel_memory: Option<i64>,
    pub memory_reservation: Option<i64>,
    pub memory_swap: Option<i64>,
    pub memory_swappiness: Option<i64>,
    pub nano_cpus: Option<i64>,
    pub oom_kill_disable: Option<bool>,
    pub init: Option<bool>,
    pub pids_limit: Option<i64>,
    pub ulimits: Vec<UpdateContainerOptionsUlimits>,
    pub cpu_count: Option<i64>,
    pub cpu_percent: Option<i64>,
    #[serde(rename = "IOMaximumIOps")]
    pub io_maximum_iops: Option<i64>,
    #[serde(rename = "IOMaximumBandwidth")]
    pub io_maximum_bandwidth: Option<i64>,
    pub restart_policy: Option<UpdateContainerOptionsRestartPolicy>,
}

/// ## Rename Container Options
///
/// Parameters used in the [Rename Container API](../struct.Docker.html#method.rename_container)
#[derive(Debug, Clone, Default)]
pub struct RenameContainerOptions {
    pub name: String,
}

impl EncodableQueryString for RenameContainerOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("name", self.name)])
    }
}

/// ## Prune Container Options
///
/// Parameters used in the [Prune Container API](../struct.Docker.html#method.prune_container)
#[derive(Debug, Clone, Default)]
pub struct PruneContainersOptions {
    pub filters: HashMap<String, String>,
}

impl EncodableQueryString for PruneContainersOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("filters", serde_json::to_string(&self.filters)?)])
    }
}

/// ## Prune Container Results
///
/// Result type for the [Prune Container API](../struct.Docker.html#method.prune_container)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct PruneContainersResults {
    pub containers_deleted: Option<Vec<String>>,
    pub space_reclaimed: i64,
}

impl<C> Docker<C>
where
    C: Connect + Sync + 'static,
{
    /// ---
    /// # List Containers
    ///
    /// Returns a list of containers.
    ///
    /// # Returns
    ///
    ///  - Vector of [Container](container/struct.Container.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::container::{ListContainersOptions};
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(ListContainersOptions{
    ///     all: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.list_containers(options);
    /// ```
    pub fn list_containers(
        &self,
        options: Option<ListContainersOptions>,
    ) -> impl Future<Item = Vec<APIContainers>, Error = Error> {
        let url = "/containers/json";

        self.process_into_value(
            url,
            Builder::new().method(Method::GET),
            options,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Create Container
    ///
    /// Prepares a container for a subsequent start operation.
    ///
    /// # Returns
    ///
    ///  - [Create Container Results](container/struct.CreateContainerResults.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::container::{CreateContainerOptions, Config};
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(CreateContainerOptions{
    ///     name: String::from("my-new-container"),
    /// });
    ///
    /// let config = Config {
    ///     image: Some(String::from("hello-world")),
    ///     cmd: vec![String::from("/hello")],
    ///     ..Default::default()
    /// };
    ///
    /// docker.create_container(options, config);
    /// ```
    pub fn create_container(
        &self,
        options: Option<CreateContainerOptions>,
        config: Config,
    ) -> impl Future<Item = CreateContainerResults, Error = Error> {
        let url = "/containers/create";

        self.process_into_value(
            url,
            Builder::new().method(Method::POST),
            options,
            Some(config),
        )
    }

    /// ---
    /// # Start Container
    ///
    /// Starts a container, after preparing it with the [Create Container
    /// API](struct.Docker.html#method.create_container).
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.start_container("hello-world", None);
    /// ```
    pub fn start_container(
        &self,
        container_name: &str,
        options: Option<StartContainerOptions>,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}/start", container_name);

        self.process_into_void(
            &url,
            Builder::new().method(Method::POST),
            options,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Stop Container
    ///
    /// Stops a container.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// use boondock::container::StopContainerOptions;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// let options = Some(StopContainerOptions{
    ///     t: 30,
    /// });
    ///
    /// docker.stop_container("hello-world", options);
    /// ```
    pub fn stop_container(
        &self,
        container_name: &str,
        options: Option<StopContainerOptions>,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}/stop", container_name);

        self.process_into_void(
            &url,
            Builder::new().method(Method::POST),
            options,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Remove Container
    ///
    /// Remove a container.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::RemoveContainerOptions;
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
    pub fn remove_container(
        &self,
        container_name: &str,
        options: Option<RemoveContainerOptions>,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}", container_name);

        self.process_into_void(
            &url,
            Builder::new().method(Method::DELETE),
            options,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Wait Container
    ///
    /// Wait for a container to stop. This is a non-blocking operation, the resulting stream will
    /// end when the container stops.
    ///
    /// # Returns
    ///
    ///  - [Wait Container Results](container/struct.WaitContainerResults.html), wrapped in a
    ///  Stream.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::WaitContainerOptions;
    ///
    /// let options = Some(WaitContainerOptions{
    ///     condition: String::from("not-running"),
    /// });
    ///
    /// docker.wait_container("hello-world", options);
    /// ```
    pub fn wait_container(
        &self,
        container_name: &str,
        options: Option<WaitContainerOptions>,
    ) -> impl Stream<Item = WaitContainerResults, Error = Error> {
        let url = format!("/containers/{}/wait", container_name);

        self.process_into_stream(
            &url,
            Builder::new().method(Method::POST),
            options,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Restart Container
    ///
    /// Restart a container.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::RestartContainerOptions;
    ///
    /// let options = Some(RestartContainerOptions{
    ///     t: 30,
    /// });
    ///
    /// docker.restart_container("postgres", options);
    /// ```
    pub fn restart_container(
        &self,
        container_name: &str,
        options: Option<RestartContainerOptions>,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}/restart", container_name);

        self.process_into_void(
            &url,
            Builder::new().method(Method::POST),
            options,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Inspect Container
    ///
    /// Inspect a container.
    ///
    /// # Returns
    ///
    ///  - [Container](container/struct.Container.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::container::{InspectContainerOptions, Config};
    ///
    /// let options = Some(InspectContainerOptions{
    ///     size: false,
    /// });
    ///
    /// docker.inspect_container("hello-world", options);
    /// ```
    pub fn inspect_container(
        &self,
        container_name: &str,
        options: Option<InspectContainerOptions>,
    ) -> impl Future<Item = Container, Error = Error> {
        let url = format!("/containers/{}/json", container_name);

        self.process_into_value(
            &url,
            Builder::new().method(Method::GET),
            options,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Top Processes
    ///
    /// List processes running inside a container.
    ///
    /// # Returns
    ///
    ///  - [TopResult](container/struct.TopResult.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::container::TopOptions;
    ///
    /// let options = Some(TopOptions{
    ///     ps_args: "aux".to_string(),
    /// });
    ///
    /// docker.top_processes("fnichol/uhttpd", options);
    /// ```
    pub fn top_processes(
        &self,
        container_name: &str,
        options: Option<TopOptions>,
    ) -> impl Future<Item = TopResult, Error = Error> {
        let url = format!("/containers/{}/top", container_name);

        self.process_into_value(
            &url,
            Builder::new().method(Method::GET),
            options,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Logs
    ///
    /// Get container logs.
    ///
    /// # Returns
    ///
    ///  - [Log Output](container/enum.LogOutput.html) enum, wrapped in a
    ///  Stream.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::LogsOptions;
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
    pub fn logs(
        &self,
        container_name: &str,
        options: Option<LogsOptions>,
    ) -> impl Stream<Item = LogOutput, Error = Error> {
        let url = format!("/containers/{}/logs", container_name);

        self.process_into_stream_string(
            &url,
            Builder::new().method(Method::GET),
            options,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Container Changes
    ///
    /// Get changes on a container's filesystem.
    ///
    /// # Returns
    ///
    ///  - An Option of Vector of [Change](container/struct.Change.html) structs, wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.container_changes("hello-world");
    /// ```
    pub fn container_changes(
        &self,
        container_name: &str,
    ) -> impl Future<Item = Option<Vec<Change>>, Error = Error> {
        let url = format!("/containers/{}/changes", container_name);

        self.process_into_value(
            &url,
            Builder::new().method(Method::GET),
            None::<NoParams>,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Stats
    ///
    /// Get container stats based on resource usage.
    ///
    /// # Returns
    ///
    ///  - [Stats](container/struct.Stats.html) struct, wrapped in a
    ///  Stream.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::StatsOptions;
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
    ) -> impl Stream<Item = Stats, Error = Error> {
        let url = format!("/containers/{}/stats", container_name);

        self.process_into_stream(
            &url,
            Builder::new().method(Method::GET),
            options,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Kill Container
    ///
    /// Kill a container.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::KillContainerOptions;
    ///
    /// let options = Some(KillContainerOptions{
    ///     signal: "SIGINT".to_string(),
    /// });
    ///
    /// docker.kill_container("postgres", options);
    /// ```
    pub fn kill_container(
        &self,
        container_name: &str,
        options: Option<KillContainerOptions>,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}/kill", container_name);

        self.process_into_void(
            &url,
            Builder::new().method(Method::POST),
            options,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Update Container
    ///
    /// Update a container.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::UpdateContainerOptions;
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
    pub fn update_container(
        &self,
        container_name: &str,
        config: UpdateContainerOptions,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}/update", container_name);

        self.process_into_void(
            &url,
            Builder::new().method(Method::POST),
            None::<NoParams>,
            Some(config),
        )
    }

    /// ---
    /// # Rename Container
    ///
    /// Rename a container.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::RenameContainerOptions;
    ///
    /// let required = RenameContainerOptions {
    ///     name: "my_new_container_name".to_string()
    /// };
    ///
    /// docker.rename_container("hello-world", required);
    /// ```
    pub fn rename_container(
        &self,
        container_name: &str,
        options: RenameContainerOptions,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}/rename", container_name);

        self.process_into_void(
            &url,
            Builder::new().method(Method::POST),
            Some(options),
            None::<NoParams>,
        )
    }

    /// ---
    /// # Pause Container
    ///
    /// Use the cgroups freezer to suspend all processes in a container.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.pause_container("postgres");
    /// ```
    pub fn pause_container(&self, container_name: &str) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}/pause", container_name);

        self.process_into_void(
            &url,
            Builder::new().method(Method::POST),
            None::<NoParams>,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Unpause Container
    ///
    /// Resume a container which has been paused.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.unpause_container("postgres");
    /// ```
    pub fn unpause_container(&self, container_name: &str) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}/unpause", container_name);

        self.process_into_void(
            &url,
            Builder::new().method(Method::POST),
            None::<NoParams>,
            None::<NoParams>,
        )
    }

    /// ---
    /// # Prune Containers
    ///
    /// Delete stopped containers.
    ///
    /// # Returns
    ///
    ///  - [PruneContainersResults](container/struct.PruneContainersResults.html) struct, wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.unpause_container("postgres");
    /// ```
    pub fn prune_containers(
        &self,
        options: Option<PruneContainersOptions>,
    ) -> impl Future<Item = PruneContainersResults, Error = Error> {
        let url = "/containers/prune";

        self.process_into_value(
            url,
            Builder::new().method(Method::POST),
            options,
            None::<NoParams>,
        )
    }
}

impl<C> DockerChain<C>
where
    C: Connect + Sync + 'static,
{
    /// ---
    /// # Kill Container
    ///
    /// Kill a container. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and the unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::KillContainerOptions;
    ///
    /// let options = Some(KillContainerOptions{
    ///     signal: "SIGINT".to_string(),
    /// });
    ///
    /// docker.chain().kill_container("postgres", options);
    /// ```
    pub fn kill_container(
        self,
        container_name: &str,
        options: Option<KillContainerOptions>,
    ) -> impl Future<Item = (DockerChain<C>, ()), Error = Error> {
        self.inner
            .kill_container(container_name, options)
            .map(|result| (self, result))
    }

    /// ---
    /// # Remove Container
    ///
    /// Remove a container. Consumes the instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and the unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::RemoveContainerOptions;
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(RemoveContainerOptions{
    ///     force: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.chain().remove_container("hello-world", options);
    /// ```
    pub fn remove_container(
        self,
        container_name: &str,
        options: Option<RemoveContainerOptions>,
    ) -> impl Future<Item = (DockerChain<C>, ()), Error = Error> {
        self.inner
            .remove_container(container_name, options)
            .map(|result| (self, result))
    }

    /// ---
    /// # Create Container
    ///
    /// Prepares a container for a subsequent start operation. Consumes the instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a
    ///  [Create Container Results](container/struct.CreateContainerResults.html), wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::container::{CreateContainerOptions, Config};
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(CreateContainerOptions{
    ///     name: String::from("my-new-container"),
    /// });
    ///
    /// let config = Config {
    ///     image: Some(String::from("hello-world")),
    ///     cmd: vec![String::from("/hello")],
    ///     ..Default::default()
    /// };
    ///
    /// docker.chain().create_container(options, config);
    /// ```
    pub fn create_container(
        self,
        options: Option<CreateContainerOptions>,
        config: Config,
    ) -> impl Future<Item = (DockerChain<C>, CreateContainerResults), Error = Error> {
        self.inner
            .create_container(options, config)
            .map(|result| (self, result))
    }

    /// ---
    /// # Start Container
    ///
    /// Starts a container, after preparing it with the [Create Container
    /// API](struct.Docker.html#method.create_container). Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and the unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.chain().start_container("hello-world", None);
    /// ```
    pub fn start_container(
        self,
        container_name: &str,
        options: Option<StartContainerOptions>,
    ) -> impl Future<Item = (DockerChain<C>, ()), Error = Error> {
        self.inner
            .start_container(container_name, options)
            .map(|result| (self, result))
    }

    /// ---
    /// # List Containers
    ///
    /// Returns a list of containers. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a Vector
    ///  of [Container](container/struct.Container.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::container::{ListContainersOptions};
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(ListContainersOptions{
    ///     all: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.chain().list_containers(options);
    /// ```
    pub fn list_containers(
        self,
        options: Option<ListContainersOptions>,
    ) -> impl Future<Item = (DockerChain<C>, Vec<APIContainers>), Error = Error> {
        self.inner
            .list_containers(options)
            .map(|result| (self, result))
    }

    /// ---
    /// # Wait Container
    ///
    /// Wait for a container to stop. This is a non-blocking operation, the resulting stream will
    /// end when the container stops. Consumes the instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a [Wait
    ///  Container Results](container/struct.WaitContainerResults.html), wrapped in a Stream.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::WaitContainerOptions;
    ///
    /// let options = Some(WaitContainerOptions{
    ///     condition: String::from("not-running"),
    /// });
    ///
    /// docker.chain().wait_container("hello-world", options);
    /// ```
    pub fn wait_container(
        self,
        container_name: &str,
        options: Option<WaitContainerOptions>,
    ) -> impl Future<
        Item = (
            DockerChain<C>,
            impl Stream<Item = WaitContainerResults, Error = Error>,
        ),
        Error = Error,
    > {
        self.inner
            .wait_container(container_name, options)
            .into_future()
            .map(|(first, rest)| match first {
                Some(head) => (self, EitherStream::A(stream::once(Ok(head)).chain(rest))),
                None => (self, EitherStream::B(stream::empty())),
            })
            .map_err(|(err, _)| err)
    }

    /// ---
    /// # Inspect Container
    ///
    /// Inspect a container. Consumes the instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a
    ///  [Container](container/struct.Container.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::container::{InspectContainerOptions, Config};
    ///
    /// let options = Some(InspectContainerOptions{
    ///     size: false,
    /// });
    ///
    /// docker.chain().inspect_container("hello-world", options);
    /// ```
    pub fn inspect_container(
        self,
        container_name: &str,
        options: Option<InspectContainerOptions>,
    ) -> impl Future<Item = (DockerChain<C>, Container), Error = Error> {
        self.inner
            .inspect_container(container_name, options)
            .map(|result| (self, result))
    }

    /// ---
    /// # Restart Container
    ///
    /// Restart a container. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and the unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::RestartContainerOptions;
    ///
    /// let options = Some(RestartContainerOptions{
    ///     t: 30,
    /// });
    ///
    /// docker.chain().restart_container("postgres", options);
    /// ```
    pub fn restart_container(
        self,
        container_name: &str,
        options: Option<RestartContainerOptions>,
    ) -> impl Future<Item = (DockerChain<C>, ()), Error = Error> {
        self.inner
            .restart_container(container_name, options)
            .map(|result| (self, result))
    }

    /// ---
    /// # Top Processes
    ///
    /// List processes running inside a container. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a
    ///  [TopResult](container/struct.TopResult.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::container::TopOptions;
    ///
    /// let options = Some(TopOptions{
    ///     ps_args: "aux".to_string(),
    /// });
    ///
    /// docker.chain().top_processes("fnichol/uhttpd", options);
    /// ```
    pub fn top_processes(
        self,
        container_name: &str,
        options: Option<TopOptions>,
    ) -> impl Future<Item = (DockerChain<C>, TopResult), Error = Error> {
        self.inner
            .top_processes(container_name, options)
            .map(|result| (self, result))
    }

    /// ---
    /// # Logs
    ///
    /// Get container logs. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a [Log
    ///  Output](container/enum.LogOutput.html) enum, wrapped in a Stream.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::LogsOptions;
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(LogsOptions{
    ///     stdout: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.chain().logs("hello-world", options);
    /// ```
    pub fn logs(
        self,
        container_name: &str,
        options: Option<LogsOptions>,
    ) -> impl Future<Item = (DockerChain<C>, impl Stream<Item = LogOutput, Error = Error>), Error = Error>
    {
        self.inner
            .logs(container_name, options)
            .into_future()
            .map(|(first, rest)| match first {
                Some(head) => (self, EitherStream::A(stream::once(Ok(head)).chain(rest))),
                None => (self, EitherStream::B(stream::empty())),
            })
            .map_err(|(err, _)| err)
    }

    /// ---
    /// # Container Changes
    ///
    /// Get changes on a container's filesystem. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and an
    ///  Option of Vector of [Change](container/struct.Change.html) structs, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.chain().container_changes("hello-world");
    /// ```
    pub fn container_changes(
        self,
        container_name: &str,
    ) -> impl Future<Item = (DockerChain<C>, Option<Vec<Change>>), Error = Error> {
        self.inner
            .container_changes(container_name)
            .map(|result| (self, result))
    }

    /// ---
    /// # Stats
    ///
    /// Get container stats based on resource usage. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a
    ///  [Stats](container/struct.Stats.html) struct, wrapped in a Stream.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::StatsOptions;
    ///
    /// let options = Some(StatsOptions{
    ///     stream: false,
    /// });
    ///
    /// docker.chain().stats("hello-world", options);
    /// ```
    pub fn stats(
        self,
        container_name: &str,
        options: Option<StatsOptions>,
    ) -> impl Future<Item = (DockerChain<C>, impl Stream<Item = Stats, Error = Error>), Error = Error>
    {
        self.inner
            .stats(container_name, options)
            .into_future()
            .map(|(first, rest)| match first {
                Some(head) => (self, EitherStream::A(stream::once(Ok(head)).chain(rest))),
                None => (self, EitherStream::B(stream::empty())),
            })
            .map_err(|(err, _)| err)
    }

    /// ---
    /// # Update Container
    ///
    /// Update a container. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and the unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::UpdateContainerOptions;
    /// use std::default::Default;
    ///
    /// let config = UpdateContainerOptions {
    ///     memory: Some(314572800),
    ///     memory_swap: Some(314572800),
    ///     ..Default::default()
    /// };
    ///
    /// docker.chain().update_container("postgres", config);
    /// ```
    pub fn update_container(
        self,
        container_name: &str,
        config: UpdateContainerOptions,
    ) -> impl Future<Item = (DockerChain<C>, ()), Error = Error> {
        self.inner
            .update_container(container_name, config)
            .map(|result| (self, result))
    }

    /// ---
    /// # Rename Container
    ///
    /// Rename a container. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and the unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use boondock::container::RenameContainerOptions;
    ///
    /// let required = RenameContainerOptions {
    ///     name: "my_new_container_name".to_string()
    /// };
    ///
    /// docker.chain().rename_container("hello-world", required);
    /// ```
    pub fn rename_container(
        self,
        container_name: &str,
        options: RenameContainerOptions,
    ) -> impl Future<Item = (DockerChain<C>, ()), Error = Error> {
        self.inner
            .rename_container(container_name, options)
            .map(|result| (self, result))
    }

    /// ---
    /// # Pause Container
    ///
    /// Use the cgroups freezer to suspend all processes in a container. Consumes the client
    /// instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and the unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.chain().pause_container("postgres");
    /// ```
    pub fn pause_container(
        self,
        container_name: &str,
    ) -> impl Future<Item = (DockerChain<C>, ()), Error = Error> {
        self.inner
            .pause_container(container_name)
            .map(|result| (self, result))
    }

    /// ---
    /// # Unpause Container
    ///
    /// Resume a container which has been paused. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and the unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.chain().unpause_container("postgres");
    /// ```
    pub fn unpause_container(
        self,
        container_name: &str,
    ) -> impl Future<Item = (DockerChain<C>, ()), Error = Error> {
        self.inner
            .unpause_container(container_name)
            .map(|result| (self, result))
    }

    /// ---
    /// # Prune Containers
    ///
    /// Delete stopped containers. Consumes the client instance.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a
    ///  [PruneContainersResults](container/struct.PruneContainersResults.html) struct, wrapped in
    ///  a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.chain().unpause_container("postgres");
    /// ```
    pub fn prune_containers(
        self,
        options: Option<PruneContainersOptions>,
    ) -> impl Future<Item = (DockerChain<C>, PruneContainersResults), Error = Error> {
        self.inner
            .prune_containers(options)
            .map(|result| (self, result))
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use hyper_mock::SequentialConnector;
    use tokio;

    #[test]
    fn test_create_container() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 89\r\n\r\n{\"Id\":\"696ce476e95d5122486cac5a446280c56aa0b02617690936e25243195992d3cc\",\"Warnings\":null}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let options = Some(CreateContainerOptions {
            name: "unit-test".to_string(),
        });

        let config = Config {
            image: Some("hello-world".to_string()),
            ..Default::default()
        };

        let results = docker.create_container(options, config);

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|result| {
                assert_eq!(
                    result.id,
                    "696ce476e95d5122486cac5a446280c56aa0b02617690936e25243195992d3cc".to_string()
                )
            });

        tokio::runtime::run(future);
    }

    #[test]
    fn test_start_container() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let results = docker.start_container("hello-world", None);

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|_| assert!(true));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_stop_container() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let results = docker.stop_container("hello-world", None);

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|_| assert!(true));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_remove_container() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let options = Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
        });

        let results = docker.remove_container("hello-world", options);

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|_| assert!(true));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_wait_container() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 29\r\n\r\n{\"Error\":null,\"StatusCode\":0}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let options = Some(WaitContainerOptions {
            condition: String::from("not-running"),
        });

        let stream = docker.wait_container("hello-world", options);

        let future = stream
            .into_future()
            .map_err(|e| panic!("error = {:?}", e.0))
            .map(|result| assert_eq!(0, result.0.unwrap().status_code));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_restart_container() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let options = Some(RestartContainerOptions { t: 30 });

        let results = docker.restart_container("hello-world", options);

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|_| assert!(true));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_inspect_container() {
        let mut connector = SequentialConnector::default();

        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 2594\r\n\r\n{\"Id\":\"156ffa6b4233d93b91dc3185b9de7225c22350d55a6db250549039a7e53efda7\",\"Created\":\"2018-10-06T15:15:43.525300512Z\",\"Path\":\"/usr/sbin/run_uhttpd\",\"Args\":[],\"State\":{\"Status\":\"running\",\"Running\":true,\"Paused\":false,\"Restarting\":false,\"OOMKilled\":false,\"Dead\":false,\"Pid\":28837,\"ExitCode\":0,\"Error\":\"\",\"StartedAt\":\"2018-10-06T15:15:54.444625149Z\",\"FinishedAt\":\"2018-10-06T15:15:53.958358249Z\"},\"Image\":\"sha256:df0db1779d4d71e169756bbcc7757f3d3d8b99032f4022c44509bf9b8f297997\",\"ResolvConfPath\":\"/home/docker/containers/156ffa6b4233d93b91dc3185b9de7225c22350d55a6db250549039a7e53efda7/resolv.conf\",\"HostnamePath\":\"/home/docker/containers/156ffa6b4233d93b91dc3185b9de7225c22350d55a6db250549039a7e53efda7/hostname\",\"HostsPath\":\"/home/docker/containers/156ffa6b4233d93b91dc3185b9de7225c22350d55a6db250549039a7e53efda7/hosts\",\"LogPath\":\"/home/docker/containers/156ffa6b4233d93b91dc3185b9de7225c22350d55a6db250549039a7e53efda7/156ffa6b4233d93b91dc3185b9de7225c22350d55a6db250549039a7e53efda7-json.log\",\"Name\":\"/integration_test_restart_container\",\"RestartCount\":0,\"Driver\":\"overlay2\",\"Platform\":\"linux\",\"MountLabel\":\"\",\"ProcessLabel\":\"\",\"AppArmorProfile\":\"docker-default\",\"ExecIDs\":null,\"HostConfig\":{},\"GraphDriver\":{\"Data\":null,\"Name\":\"overlay2\"},\"Mounts\":[],\"Config\":{\"Hostname\":\"156ffa6b4233\",\"Domainname\":\"\",\"User\":\"\",\"AttachStdin\":false,\"AttachStdout\":false,\"AttachStderr\":false,\"ExposedPorts\":{\"80/tcp\":{}},\"Tty\":false,\"OpenStdin\":false,\"StdinOnce\":false,\"Env\":[],\"Cmd\":[],\"Image\":\"fnichol/uhttpd\",\"Volumes\":{\"/www\":{}},\"WorkingDir\":\"\",\"Entrypoint\":[\"/usr/sbin/run_uhttpd\",\"-f\",\"-p\",\"80\",\"-h\",\"/www\"],\"OnBuild\":null,\"Labels\":{}},\"NetworkSettings\":{\"Bridge\":\"\",\"SandboxID\":\"20cd513ef83bc14934be89953d22aab5a54c7769b07c8e93e90f0227d0aba96b\",\"HairpinMode\":false,\"LinkLocalIPv6Address\":\"\",\"LinkLocalIPv6PrefixLen\":0,\"Ports\":{\"80/tcp\":null},\"SandboxKey\":\"/var/run/docker/netns/20cd513ef83b\",\"SecondaryIPAddresses\":null,\"SecondaryIPv6Addresses\":null,\"EndpointID\":\"992f7e94fd721f627d9d1611c27b477d39b959c209286c38426215ea764f6d63\",\"Gateway\":\"172.17.0.1\",\"GlobalIPv6Address\":\"\",\"GlobalIPv6PrefixLen\":0,\"IPAddress\":\"172.17.0.3\",\"IPPrefixLen\":16,\"IPv6Gateway\":\"\",\"MacAddress\":\"02:42:ac:11:00:03\",\"Networks\":{\"bridge\":{\"IPAMConfig\":null,\"Links\":null,\"Aliases\":null,\"NetworkID\":\"424a1638d72f8984c670bc8bf269102360f24bd356188635ab359cb0b0792b20\",\"EndpointID\":\"992f7e94fd721f627d9d1611c27b477d39b959c209286c38426215ea764f6d63\",\"Gateway\":\"172.17.0.1\",\"IPAddress\":\"172.17.0.3\",\"IPPrefixLen\":16,\"IPv6Gateway\":\"\",\"GlobalIPv6Address\":\"\",\"GlobalIPv6PrefixLen\":0,\"MacAddress\":\"02:42:ac:11:00:03\",\"DriverOpts\":null}}}}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let results = docker.inspect_container("uhttpd", None);

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|result| assert_eq!(result.path, "/usr/sbin/run_uhttpd".to_string()));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_top_processes() {
        let mut connector = SequentialConnector::default();

        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 243\r\n\r\n{\"Processes\":[[\"root\",\"3773\",\"0.0\",\"0.0\",\"11056\",\"348\",\"?\",\"Ss\",\"19:42\",\"0:00\",\"/usr/sbin/uhttpd -f -p 80 -h /www /usr/sbin/run_uhttpd -f -p 80 -h /www\"]],\"Titles\":[\"USER\",\"PID\",\"%CPU\",\"%MEM\",\"VSZ\",\"RSS\",\"TTY\",\"STAT\",\"START\",\"TIME\",\"COMMAND\"]}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let results = docker.top_processes(
            "uhttpd",
            Some(TopOptions {
                ps_args: "aux".to_string(),
            }),
        );

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|result| assert_eq!(result.titles[0], "USER".to_string()));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_logs() {
        let mut connector = SequentialConnector::default();

        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 26\r\n\r\n\u{1}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{13}Hello from Docker!
            \r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let stream = docker.logs(
            "hello-world",
            Some(LogsOptions {
                stdout: true,
                ..Default::default()
            }),
        );

        let future = stream
            .into_future()
            .map_err(|e| panic!("error = {:?}", e.0))
            .map(|result| {
                assert_eq!(
                    format!("{}", result.0.unwrap()),
                    "Hello from Docker!".to_string()
                )
            });

        tokio::runtime::run(future);
    }

    #[test]
    fn test_container_changes() {
        let mut connector = SequentialConnector::default();

        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 4\r\n\r\nnull\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let result = docker.container_changes("hello-world");

        let future = result
            .map_err(|e| panic!("error = {:?}", e))
            .map(|result| assert!(result.is_none()));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_stats() {
        let mut connector = SequentialConnector::default();

        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 1862\r\n\r\n{\"read\":\"2018-10-19T06:11:22.220728356Z\",\"preread\":\"2018-10-19T06:11:21.218466258Z\",\"pids_stats\":{\"current\":1},\"blkio_stats\":{\"io_service_bytes_recursive\":[],\"io_serviced_recursive\":[],\"io_queue_recursive\":[],\"io_service_time_recursive\":[],\"io_wait_time_recursive\":[],\"io_merged_recursive\":[],\"io_time_recursive\":[],\"sectors_recursive\":[]},\"num_procs\":0,\"storage_stats\":{},\"cpu_stats\":{\"cpu_usage\":{\"total_usage\":23097208,\"percpu_usage\":[709093,1595689,5032998,15759428],\"usage_in_kernelmode\":0,\"usage_in_usermode\":10000000},\"system_cpu_usage\":4447677200000000,\"online_cpus\":4,\"throttling_data\":{\"periods\":0,\"throttled_periods\":0,\"throttled_time\":0}},\"precpu_stats\":{\"cpu_usage\":{\"total_usage\":23097208,\"percpu_usage\":[709093,1595689,5032998,15759428],\"usage_in_kernelmode\":0,\"usage_in_usermode\":10000000},\"system_cpu_usage\":4447673150000000,\"online_cpus\":4,\"throttling_data\":{\"periods\":0,\"throttled_periods\":0,\"throttled_time\":0}},\"memory_stats\":{\"usage\":962560,\"max_usage\":5406720,\"stats\":{\"active_anon\":86016,\"active_file\":0,\"cache\":0,\"dirty\":0,\"hierarchical_memory_limit\":9223372036854771712,\"hierarchical_memsw_limit\":0,\"inactive_anon\":0,\"inactive_file\":0,\"mapped_file\":0,\"pgfault\":1485,\"pgmajfault\":0,\"pgpgin\":1089,\"pgpgout\":1084,\"rss\":0,\"rss_huge\":0,\"total_active_anon\":86016,\"total_active_file\":0,\"total_cache\":0,\"total_dirty\":0,\"total_inactive_anon\":0,\"total_inactive_file\":0,\"total_mapped_file\":0,\"total_pgfault\":1485,\"total_pgmajfault\":0,\"total_pgpgin\":1089,\"total_pgpgout\":1084,\"total_rss\":0,\"total_rss_huge\":0,\"total_unevictable\":0,\"total_writeback\":0,\"unevictable\":0,\"writeback\":0},\"limit\":16750219264},\"name\":\"/integration_test_stats\",\"id\":\"66667eab5737dda2da2f578e9496e45c074d1bc5badc0484314f1c3afccfaeb0\",\"networks\":{\"eth0\":{\"rx_bytes\":1635,\"rx_packets\":14,\"rx_errors\":0,\"rx_dropped\":0,\"tx_bytes\":0,\"tx_packets\":0,\"tx_errors\":0,\"tx_dropped\":0}}}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let stream = docker.stats("hello-world", Some(StatsOptions { stream: false }));

        let future = stream
            .into_future()
            .map_err(|e| panic!("error = {:?}", e.0))
            .map(|result| assert_eq!(result.0.unwrap().pids_stats.current.unwrap(), 1));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_kill_container() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let options = Some(KillContainerOptions {
            signal: "SIGKILL".to_string(),
        });

        let results = docker.kill_container("postgres", options);

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|_| assert!(true));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_update_container() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let config = UpdateContainerOptions {
            memory: Some(314572800),
            memory_swap: Some(314572800),
            ..Default::default()
        };

        let results = docker.update_container("postgres", config);

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|_| assert!(true));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_rename_container() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let options = RenameContainerOptions {
            name: "my_new_container_name".to_string(),
        };

        let results = docker.rename_container("postgres", options);

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|_| assert!(true));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_pause_container() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let results = docker.pause_container("postgres");

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|_| assert!(true));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_unpause_container() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let results = docker.unpause_container("postgres");

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|_| assert!(true));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_prune_containers() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 41\r\n\r\n{\"ImagesDeleted\":null,\"SpaceReclaimed\":0}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let results = docker.prune_containers(None);

        let future = results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|_| assert!(true));

        tokio::runtime::run(future);
    }
}
