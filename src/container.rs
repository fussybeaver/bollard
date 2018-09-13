use failure::Error;
use futures::Stream;
use hyper::client::connect::Connect;
use hyper::rt::Future;
use hyper::Method;

use std::collections::HashMap;

use super::Docker;
use options::{EncodableQueryString, NoParams};

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
    #[serde(rename = "NanoCPUs")]
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
    pub ulimits: Vec<HashMap<String, String>>,
    pub log_config: Option<LogConfig>,
    pub security_opt: Option<Vec<String>>,
    pub storage_opt: Option<HashMap<String, String>>,
    pub cgroup_parent: Option<String>,
    pub volume_driver: Option<String>,
    pub shm_size: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
#[allow(non_snake_case)]
pub struct LogConfig {
    pub Type: Option<String>,
    pub config: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkingConfig {
    pub endpoints_config: HashMap<String, EndpointConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointIPAMConfig {
    #[serde(rename = "IPV4Address")]
    ipv4_address: String,
    #[serde(rename = "IPV6Address")]
    ipv6_address: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct EndpointConfig {
    #[serde(rename = "IPAMConfig")]
    ipam_config: Option<EndpointIPAMConfig>,
    links: Option<Vec<String>>,
    aliases: Option<Vec<String>>,
    #[serde(rename = "NetworkID")]
    network_id: Option<String>,
    #[serde(rename = "EndpointID")]
    endpoint_id: Option<String>,
    gateway: Option<String>,
    ip_address: Option<String>,
    ip_prefix_len: Option<u64>,
    #[serde(rename = "IPv6Gateway")]
    ipv6_gateway: Option<String>,
    #[serde(rename = "GlobalIPv6Address")]
    global_ipv6_address: Option<String>,
    #[serde(rename = "GlobalIPv6PrefixLen")]
    global_ipv6_prefix_len: Option<u64>,
    mac_address: Option<String>,
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
    pub tty: Option<bool>,
    pub open_stdin: Option<bool>,
    pub stdin_once: Option<bool>,
    pub env: Option<Vec<String>>,
    pub cmd: Vec<String>,
    pub entrypoint: String,
    pub image: Option<String>,
    pub labels: Option<HashMap<String, String>>,
    pub volumes: Option<HashMap<String, String>>,
    pub working_dir: Option<String>,
    pub network_disabled: Option<bool>,
    pub mac_address: Option<String>,
    pub exposed_ports: Option<HashMap<String, HashMap<(), ()>>>,
    pub stop_signal: Option<String>,
    pub stop_timeout: Option<usize>,
    pub host_config: Option<HostConfig>,
    pub networking_config: Option<NetworkingConfig>,
}

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
    pub detach_keys: u64,
}

impl EncodableQueryString for StartContainerOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("detachKeys", self.detach_keys.to_string())])
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct WaitContainerResults {
    pub status_code: u16,
    pub error: Option<WaitContainerResultsError>,
}

impl<C> Docker<C>
where
    C: Connect + Sync + 'static,
{
    pub fn create_container(
        &self,
        options: Option<CreateContainerOptions>,
        config: Config,
    ) -> impl Future<Item = CreateContainerResults, Error = Error> {
        let url = "/containers/create";

        self.process_into_value(url, Method::POST, options, Some(config))
    }

    pub fn start_container(
        &self,
        container_name: &str,
        options: Option<StartContainerOptions>,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}/start", container_name);

        self.process_into_void(&url, Method::POST, options, None::<NoParams>)
    }

    pub fn stop_container(
        &self,
        container_name: &str,
        options: Option<StopContainerOptions>,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}/stop", container_name);

        self.process_into_void(&url, Method::POST, options, None::<NoParams>)
    }

    pub fn remove_container(
        &self,
        container_name: &str,
        options: Option<RemoveContainerOptions>,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/containers/{}", container_name);

        self.process_into_void(&url, Method::DELETE, options, None::<NoParams>)
    }

    pub fn wait_container(
        &self,
        container_name: &str,
        options: Option<WaitContainerOptions>,
    ) -> impl Stream<Item = WaitContainerResults, Error = Error> {
        let url = format!("/containers/{}/wait", container_name);

        self.process_into_stream(&url, Method::POST, options, None::<NoParams>)
    }
}
