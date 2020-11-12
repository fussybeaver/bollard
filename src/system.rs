//! System API: interface for interacting with the Docker server and/or Registry.

use chrono::{DateTime, Utc};
use futures_core::Stream;
use http::request::Builder;
use hyper::{Body, Method};
use serde::ser::Serialize;
use serde_json::value::Value;

use std::collections::HashMap;
use std::hash::Hash;

use super::Docker;
use crate::errors::Error;
use crate::models::*;

/// Response of Engine API: GET \"/version\"
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct Version {
    #[serde(rename = "Platform")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<SystemVersionPlatform>,

    /// Information about system components
    #[serde(rename = "Components")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<VersionComponents>>,

    /// The version of the daemon
    #[serde(rename = "Version")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// The default (and highest) API version that is supported by the daemon
    #[serde(rename = "ApiVersion")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,

    /// The minimum API version that is supported by the daemon
    #[serde(rename = "MinAPIVersion")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_api_version: Option<String>,

    /// The Git commit of the source code that was used to build the daemon
    #[serde(rename = "GitCommit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,

    /// The version Go used to compile the daemon, and the version of the Go runtime in use.
    #[serde(rename = "GoVersion")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go_version: Option<String>,

    /// The operating system that the daemon is running on (\"linux\" or \"windows\")
    #[serde(rename = "Os")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,

    /// The architecture that the daemon is running on
    #[serde(rename = "Arch")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,

    /// The kernel version (`uname -r`) that the daemon is running on.  This field is omitted when empty.
    #[serde(rename = "KernelVersion")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,

    /// Indicates if the daemon is started with experimental features enabled.  This field is omitted when empty / false.
    #[serde(rename = "Experimental")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg(windows)]
    pub experimental: Option<bool>,
    #[cfg(not(windows))]
    pub experimental: Option<String>,

    /// The date and time that the daemon was compiled.
    #[serde(rename = "BuildTime")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct VersionComponents {
    /// Name of the component
    #[serde(rename = "Name")]
    pub name: String,

    /// Version of the component
    #[serde(rename = "Version")]
    pub version: String,

    /// Key/value pairs of strings with additional information about the component. These values are intended for informational purposes only, and their content is not defined, and not part of the API specification.  These messages can be printed by the client as information to the user.
    #[serde(rename = "Details")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, Value>>,
}

/// Response of Engine API: GET \"/info\"
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct Info {
    #[serde(rename = "ID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "Containers")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub containers: Option<usize>,

    #[serde(rename = "ContainersRunning")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub containers_running: Option<usize>,

    #[serde(rename = "ContainersPaused")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub containers_paused: Option<usize>,

    #[serde(rename = "ContainersStopped")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub containers_stopped: Option<usize>,

    #[serde(rename = "Images")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<usize>,

    #[serde(rename = "Driver")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,

    #[serde(rename = "DriverStatus")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_status: Option<Vec<Vec<String>>>,

    #[serde(rename = "SystemStatus")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_status: Option<String>,

    #[serde(rename = "Plugins")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<InfoPlugins>,

    #[serde(rename = "MemoryLimit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<bool>,

    #[serde(rename = "SwapLimit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_limit: Option<bool>,

    #[serde(rename = "KernelMemoryTCP")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_memory: Option<bool>,

    #[serde(rename = "CpuCfsPeriod")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cfs_period: Option<bool>,

    #[serde(rename = "MemoryLimit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cfs_quota: Option<bool>,

    #[serde(rename = "CPUShares")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpushares: Option<bool>,

    #[serde(rename = "CPUSet")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpuset: Option<bool>,

    #[serde(rename = "PidsLimit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids_limit: Option<bool>,

    #[serde(rename = "IPv4Forwarding")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4_forwarding: Option<bool>,

    #[serde(rename = "BridgeNfIptables")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_nf_iptables: Option<bool>,

    #[serde(rename = "BridgeNfIp6tables")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_nf_ip6_tables: Option<bool>,

    #[serde(rename = "Debug")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,

    #[serde(rename = "NFd")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfd: Option<usize>,

    #[serde(rename = "OomKillDisable")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oom_kill_disable: Option<bool>,

    #[serde(rename = "NGoroutines")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ngoroutines: Option<usize>,

    #[serde(rename = "SystemTime")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_time: Option<String>,

    #[serde(rename = "LoggingDriver")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging_driver: Option<String>,

    #[serde(rename = "CgroupDriver")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgroup_driver: Option<String>,

    #[serde(rename = "NEventsListener")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nevents_listener: Option<usize>,

    #[serde(rename = "KernelVersion")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,

    #[serde(rename = "OperatingSystem")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operating_system: Option<String>,

    #[serde(rename = "Ostype")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ostype: Option<String>,

    #[serde(rename = "Architecture")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,

    #[serde(rename = "CgroupDriver")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_server_address: Option<String>,

    #[serde(rename = "RegistryConfig")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_config: Option<InfoRegistryConfig>,

    #[serde(rename = "NCPU")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ncpu: Option<usize>,

    #[serde(rename = "MemTotal")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_total: Option<usize>,

    #[serde(rename = "GenericResources")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generic_resources: Option<String>,

    #[serde(rename = "DockerRootDir")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_root_dir: Option<String>,

    #[serde(rename = "HttpProxy")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_proxy: Option<String>,

    #[serde(rename = "HttpsProxy")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub https_proxy: Option<String>,

    #[serde(rename = "NoProxy")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_proxy: Option<String>,

    #[serde(rename = "Name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(rename = "Labels")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,

    #[serde(rename = "ExperimentalBuild")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental_build: Option<bool>,

    #[serde(rename = "ServerVersion")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,

    #[serde(rename = "ClusterStore")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_store: Option<String>,

    #[serde(rename = "ClusterAdvertise")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_advertise: Option<String>,

    #[serde(rename = "Runtimes")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtimes: Option<InfoRuntimes>,

    #[serde(rename = "DefaultRuntime")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_runtime: Option<String>,

    #[serde(rename = "Swarm")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swarm: Option<InfoSwarm>,

    #[serde(rename = "LiveRestoreEnabled")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_restore_enabled: Option<bool>,

    #[serde(rename = "Isolation")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isolation: Option<String>,

    #[serde(rename = "InitBinary")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init_binary: Option<String>,

    #[serde(rename = "ContainerdCommit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub containerd_commit: Option<InfoContainerdCommit>,

    #[serde(rename = "RuncCommit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runc_commit: Option<InfoRuncCommit>,

    #[serde(rename = "InitCommit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init_commit: Option<InfoInitCommit>,

    #[serde(rename = "SecurityOptions")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_options: Option<Vec<String>>,

    #[serde(rename = "ProductLicense")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_license: Option<String>,

    #[serde(rename = "Warnings")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct InfoPlugins {
    #[serde(rename = "Volume")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<Vec<String>>,

    #[serde(rename = "Network")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<Vec<String>>,

    #[serde(rename = "Authorization")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<String>,

    #[serde(rename = "Log")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<Vec<String>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct InfoRegistryConfig {
    #[serde(rename = "AllowNondistributableArtifactsCidrs")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_nondistributable_artifacts_cidrs: Option<Vec<String>>,

    #[serde(rename = "AllowNondistributableArtifactsHostnames")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_nondistributable_artifacts_hostnames: Option<Vec<String>>,

    #[serde(rename = "InsecureRegistryCidrs")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insecure_registry_cidrs: Option<Vec<String>>,

    #[serde(rename = "IndexConfigs")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_configs: Option<InfoIndexConfigs>,

    #[serde(rename = "Mirrors")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirrors: Option<Vec<String>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct InfoIndexConfigs {
    #[serde(rename = "DockerIo")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_io: Option<InfoDockerIo>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct InfoDockerIo {
    #[serde(rename = "Name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(rename = "Mirrors")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirros: Option<Vec<String>>,

    #[serde(rename = "Secure")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure: Option<bool>,

    #[serde(rename = "Official")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official: Option<bool>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct InfoRuntimes {
    #[serde(rename = "Run")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<InfoRunc>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct InfoRunc {
    #[serde(rename = "Path")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct InfoSwarm {
    #[serde(rename = "NodeID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,

    #[serde(rename = "NodeAddr")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_addr: Option<String>,

    #[serde(rename = "LocalNodeState")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_node_state: Option<String>,

    #[serde(rename = "ControlAvailable")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_available: Option<bool>,

    #[serde(rename = "Error")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    #[serde(rename = "RemoteManagers")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_managers: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct InfoContainerdCommit {
    #[serde(rename = "ID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "Expected")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct InfoRuncCommit {
    #[serde(rename = "ID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "Expected")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct InfoInitCommit {
    #[serde(rename = "ID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "Expected")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
}

/// Parameters used in the [Events API](../struct.Docker.html#method.events)
///
/// ## Examples
///
/// ```rust
/// # extern crate chrono;
/// use bollard::system::EventsOptions;
/// use chrono::{Duration, Utc};
/// use std::collections::HashMap;
///
/// # fn main() {
/// EventsOptions::<String>{
///     since: Some(Utc::now() - Duration::minutes(20)),
///     until: Some(Utc::now()),
///     filters: HashMap::new()
/// };
/// # }
/// ```
#[derive(Debug, Default, Clone, Serialize)]
pub struct EventsOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
{
    /// Show events created since this timestamp then stream new events.
    #[serde(serialize_with = "crate::docker::serialize_as_timestamp")]
    pub since: Option<DateTime<Utc>>,
    /// Show events created until this timestamp then stop streaming.
    #[serde(serialize_with = "crate::docker::serialize_as_timestamp")]
    pub until: Option<DateTime<Utc>>,
    /// A JSON encoded value of filters (a `map[string][]string`) to process on the event list. Available filters:
    ///  - `config=<string>` config name or ID
    ///  - `container=<string>` container name or ID
    ///  - `daemon=<string>` daemon name or ID
    ///  - `event=<string>` event type
    ///  - `image=<string>` image name or ID
    ///  - `label=<string>` image or container label
    ///  - `network=<string>` network name or ID
    ///  - `node=<string>` node ID
    ///  - `plugin`= plugin name or ID
    ///  - `scope`= local or swarm
    ///  - `secret=<string>` secret name or ID
    ///  - `service=<string>` service name or ID
    ///  - `type=<string>` object to filter by, one of `container`, `image`, `volume`, `network`, `daemon`, `plugin`, `node`, `service`, `secret` or `config`
    ///  - `volume=<string>` volume name
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
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
    ///  - [Version](system/struct.Version.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.version();
    /// ```
    pub async fn version(&self) -> Result<Version, Error> {
        let req = self.build_request(
            "/version",
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
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
    ///  - [Info](system/struct.Info.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.info();
    /// ```
    pub async fn info(&self) -> Result<Info, Error> {
        let req = self.build_request(
            "/info",
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Ping
    ///
    /// This is a dummy endpoint you can use to test if the server is accessible.
    ///
    /// # Returns
    ///
    ///  - A String, wrapped in a Future.
    ///
    /// # Examples
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
            &url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Events
    ///
    /// Stream real-time events from the server.
    ///
    /// # Returns
    ///
    ///  - [System Events Response](models/struct.SystemEventsResponse.html),
    ///  wrapped in a Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bollard::system::EventsOptions;
    /// use chrono::{Duration, Utc};
    /// use std::collections::HashMap;
    ///
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.events(Some(EventsOptions::<String> {
    ///     since: Some(Utc::now() - Duration::minutes(20)),
    ///     until: Some(Utc::now()),
    ///     filters: HashMap::new(),
    /// }));
    /// ```
    pub fn events<T>(
        &self,
        options: Option<EventsOptions<T>>,
    ) -> impl Stream<Item = Result<SystemEventsResponse, Error>>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/events";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            options,
            Ok(Body::empty()),
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
    ///  Response](models/struct.SystemDataUsageResponse.html), wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.df();
    /// ```
    pub async fn df(&self) -> Result<SystemDataUsageResponse, Error> {
        let url = "/system/df";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }
}
