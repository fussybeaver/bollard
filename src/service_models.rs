//! Service API object definitions

use chrono::DateTime;
use chrono::Utc;
use std::{collections::HashMap, hash::Hash};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct Service<T>
where
    T: AsRef<str> + Eq + Hash,
{
    #[serde(rename = "ID")]
    pub id: T,
    pub version: ObjectVersion,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub spec: ServiceSpec<T>,
    pub endpoint: ServiceEndpoint<T>,
    pub update_status: Option<ServiceUpdateStatus<T>>,
}

/// The version number of the object such as node, service, etc. This is needed to avoid conflicting writes. The client must send the version number along with the modified specification when updating these objects. This approach ensures safe concurrency and determinism in that the change on the object may not be applied if the version number has changed from the last read. In other words, if two update requests specify the same base version, only one of the requests can succeed. As a result, two separate update requests that happen at the same time will not unintentionally overwrite each other.
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ObjectVersion {
    pub index: u64,
}

/// User modifiable configuration for a service.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ServiceSpec<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Name of the service.
    pub name: T,
    /// User-defined key/value metadata.
    pub labels: HashMap<T, String>,
    pub task_template: TaskSpec<T>,
    pub mode: Option<ServiceSpecMode>,
    pub update_config: Option<ServiceSpecUpdateConfig>,
    pub rollback_config: Option<ServiceSpecUpdateConfig>,
    /// Specifies which networks the service should attach to.
    pub networks: Option<Vec<NetworkAttachmentConfig<T>>>,
    pub endpoint_spec: Option<EndpointSpec<T>>,
}

/// User modifiable task configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpec<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub plugin_spec: Option<TaskSpecPluginSpec<T>>,
    pub container_spec: Option<TaskSpecContainerSpec<T>>,
    pub network_attachment_spec: Option<TaskSpecNetworkAttachmentSpec<T>>,
    pub resources: Option<TaskSpecResources<T>>,
    pub restart_policy: Option<TaskSpecRestartPolicy>,
    pub placement: Option<TaskSpecPlacement<T>>,
    /// A counter that triggers an update even if no relevant parameters have been changed.
    pub force_update: Option<isize>,
    /// Runtime is the type of runtime specified for the task executor.
    pub runtime: Option<T>,
    /// Specifies which networks the service should attach to.
    pub networks: Option<Vec<NetworkAttachmentConfig<T>>>,
    pub log_driver: Option<TaskSpecLogDriver<T>>,
}

/// Plugin spec for the service.  *(Experimental release only.)*  <p><br /></p>  > **Note**: ContainerSpec, NetworkAttachmentSpec, and PluginSpec are > mutually exclusive. PluginSpec is only used when the Runtime field > is set to `plugin`. NetworkAttachmentSpec is used when the Runtime > field is set to `attachment`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecPluginSpec<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// The name or 'alias' to use for the plugin.
    pub name: Option<T>,
    /// The plugin image reference to use.
    pub remote: Option<T>,
    /// Disable the plugin once scheduled.
    pub disabled: Option<bool>,
    pub plugin_privilege: Option<Vec<Body<T>>>,
}

/// Describes a permission accepted by the user upon installing the plugin.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct Body<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub name: Option<T>,
    pub description: Option<T>,
    pub value: Option<Vec<T>>,
}

/// Container spec for the service.  <p><br /></p>  > **Note**: ContainerSpec, NetworkAttachmentSpec, and PluginSpec are > mutually exclusive. PluginSpec is only used when the Runtime field > is set to `plugin`. NetworkAttachmentSpec is used when the Runtime > field is set to `attachment`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecContainerSpec<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// The image name to use for the container
    pub image: Option<T>,
    /// User-defined key/value data.
    pub labels: Option<HashMap<T, String>>,
    /// The command to be run in the image.
    pub command: Option<Vec<T>>,
    /// Arguments to the command.
    pub args: Option<Vec<T>>,
    /// The hostname to use for the container, as a valid RFC 1123 hostname.
    pub hostname: Option<T>,
    /// A list of environment variables in the form `VAR=value`.
    pub env: Option<Vec<T>>,
    /// The working directory for commands to run in.
    pub dir: Option<T>,
    /// The user inside the container.
    pub user: Option<T>,
    /// A list of additional groups that the container process will run as.
    pub groups: Option<Vec<T>>,
    pub privileges: Option<TaskSpecContainerSpecPrivileges<T>>,
    /// Whether a pseudo-TTY should be allocated.
    #[serde(rename = "TTY")]
    pub tty: Option<bool>,
    /// Open `stdin`
    pub open_stdin: Option<bool>,
    /// Mount the container's root filesystem as read only.
    pub read_only: Option<bool>,
    /// Specification for mounts to be added to containers created as part of the service.
    pub mounts: Option<Vec<Mount<T>>>,
    /// Signal to stop the container.
    pub stop_signal: Option<T>,
    /// Amount of time to wait for the container to terminate before forcefully killing it.
    pub stop_grace_period: Option<i64>,
    pub health_check: Option<HealthConfig<T>>,
    /// A list of hostname/IP mappings to add to the container's `hosts` file. The format of extra hosts is specified in the [hosts(5)](http://man7.org/linux/man-pages/man5/hosts.5.html) man page:      IP_address canonical_hostname [aliases...]
    pub hosts: Option<Vec<T>>,
    #[serde(rename = "DNSConfig")]
    pub dns_config: Option<TaskSpecContainerSpecDnsConfig<T>>,
    /// Secrets contains references to zero or more secrets that will be exposed to the service.
    pub secrets: Option<Vec<TaskSpecContainerSpecSecrets<T>>>,
    /// Configs contains references to zero or more configs that will be exposed to the service.
    pub configs: Option<Vec<TaskSpecContainerSpecConfigs<T>>>,
    /// Isolation technology of the containers running the service. (Windows only)
    pub isolation: Option<TaskSpecContainerSpecIsolation>,
    /// Run an init inside the container that forwards signals and reaps processes. This field is omitted if empty, and the default (as configured on the daemon) is used.
    pub init: Option<bool>,
    /// Set kernel namedspaced parameters (sysctls) in the container. The Sysctls option on services accepts the same sysctls as the are supported on containers. Note that while the same sysctls are supported, no guarantees or checks are made about their suitability for a clustered environment, and it's up to the user to determine whether a given sysctl will work properly in a Service.
    pub sysctls: Option<HashMap<T, String>>,
}

/// Isolation technology of the containers running the service. (Windows only)
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum TaskSpecContainerSpecIsolation {
    Default,
    Process,
    Hyperv,
}

/// Security options for the container
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecContainerSpecPrivileges<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub credential_spec: Option<TaskSpecContainerSpecPrivilegesCredentialSpec<T>>,
    #[serde(rename = "SELinuxContext")]
    pub se_linux_context: Option<TaskSpecContainerSpecPrivilegesSeLinuxContext<T>>,
}

/// CredentialSpec for managed service account (Windows only)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecContainerSpecPrivilegesCredentialSpec<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Load credential spec from a Swarm Config with the given ID. The specified config must also be present in the Configs field with the Runtime property set.  <p><br /></p>   > **Note**: `CredentialSpec.File`, `CredentialSpec.Registry`, and `CredentialSpec.Config` are mutually exclusive.
    pub config: Option<T>,
    /// Load credential spec from this file. The file is read by the daemon, and must be present in the `CredentialSpecs` subdirectory in the docker data directory, which defaults to `C:\\ProgramData\\Docker\\` on Windows.  For example, specifying `spec.json` loads `C:\\ProgramData\\Docker\\CredentialSpecs\\spec.json`.  <p><br /></p>  > **Note**: `CredentialSpec.File`, `CredentialSpec.Registry`, and `CredentialSpec.Config` are mutually exclusive.
    pub file: Option<T>,
    /// Load credential spec from this value in the Windows registry. The specified registry value must be located in:  `HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Virtualization\\Containers\\CredentialSpecs`  <p><br /></p>   > **Note**: `CredentialSpec.File`, `CredentialSpec.Registry`, and `CredentialSpec.Config` are mutually exclusive.
    pub registry: Option<T>,
}

/// SELinux labels of the container
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecContainerSpecPrivilegesSeLinuxContext<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Disable SELinux
    pub disable: Option<bool>,
    /// SELinux user label
    pub user: Option<T>,
    /// SELinux role label
    pub role: Option<T>,
    /// SELinux type label
    #[serde(rename = "Type")]
    pub _type: Option<T>,
    /// SELinux level label
    pub level: Option<T>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct Mount<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Container path.
    pub target: Option<T>,
    /// Mount source (e.g. a volume name, a host path).
    pub source: Option<T>,
    /// The mount type. Available types:  - `bind` Mounts a file or directory from the host into the container. Must exist prior to creating the container. - `volume` Creates a volume with the given name and options (or uses a pre-existing volume with the same name and options). These are **not** removed when the container is removed. - `tmpfs` Create a tmpfs with the given options. The mount source cannot be specified for tmpfs. - `npipe` Mounts a named pipe from the host into the container. Must exist prior to creating the container.
    #[serde(rename = "Type")]
    pub _type: Option<MountType>,
    /// Whether the mount should be read-only.
    pub read_only: Option<bool>,
    /// The consistency requirement for the mount: `default`, `consistent`, `cached`, or `delegated`.
    pub consistency: Option<T>,
    pub bind_options: Option<MountBindOptions>,
    pub volume_options: Option<MountVolumeOptions<T>>,
    pub tmpfs_options: Option<MountTmpfsOptions>,
}

/// The mount type. Available types:  - `bind` Mounts a file or directory from the host into the container. Must exist prior to creating the container. - `volume` Creates a volume with the given name and options (or uses a pre-existing volume with the same name and options). These are **not** removed when the container is removed. - `tmpfs` Create a tmpfs with the given options. The mount source cannot be specified for tmpfs. - `npipe` Mounts a named pipe from the host into the container. Must exist prior to creating the container.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum MountType {
    Bind,
    Volume,
    Tmpfs,
    Npipe,
}

/// Optional configuration for the `bind` type.
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct MountBindOptions {
    /// A propagation mode with the value `[r]private`, `[r]shared`, or `[r]slave`.
    pub propagation: Option<MountBindOptionsPropagation>,
    /// Disable recursive bind mount.
    pub non_recursive: Option<bool>,
}

/// A propagation mode with the value `[r]private`, `[r]shared`, or `[r]slave`.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum MountBindOptionsPropagation {
    Private,
    #[serde(rename = "rprivate")]
    RPrivate,
    Shared,
    #[serde(rename = "rshared")]
    RShared,
    Slave,
    #[serde(rename = "rslave")]
    RSlave,
}

/// Optional configuration for the `volume` type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct MountVolumeOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Populate volume with data from the target.
    pub no_copy: Option<bool>,
    /// User-defined key/value metadata.
    pub labels: Option<HashMap<T, T>>,
    pub driver_config: Option<MountVolumeOptionsDriverConfig<T>>,
}

/// Map of driver specific options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct MountVolumeOptionsDriverConfig<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Name of the driver to use to create the volume.
    pub name: Option<T>,
    /// key/value map of driver specific options.
    pub options: Option<HashMap<T, T>>,
}

/// Optional configuration for the `tmpfs` type.
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct MountTmpfsOptions {
    /// The size for the tmpfs mount in bytes.
    pub size_bytes: Option<i64>,
    /// The permission mode for the tmpfs mount in an integer.
    pub mode: Option<isize>,
}

/// A test to perform to check that the container is healthy.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct HealthConfig<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// The test to perform. Possible values are:  - `[]` inherit healthcheck from image or parent image - `[\"NONE\"]` disable healthcheck - `[\"CMD\", args...]` exec arguments directly - `[\"CMD-SHELL\", command]` run command with system's default shell
    pub test: Option<Vec<T>>,
    /// The time to wait between checks in nanoseconds. It should be 0 or at least 1000000 (1 ms). 0 means inherit.
    pub interval: Option<isize>,
    /// The time to wait before considering the check to have hung. It should be 0 or at least 1000000 (1 ms). 0 means inherit.
    pub timeout: Option<isize>,
    /// The number of consecutive failures needed to consider a container as unhealthy. 0 means inherit.
    pub retries: Option<isize>,
    /// Start period for the container to initialize before starting health-retries countdown in nanoseconds. It should be 0 or at least 1000000 (1 ms). 0 means inherit.
    pub start_period: Option<isize>,
}

/// Specification for DNS related configurations in resolver configuration file (`resolv.conf`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecContainerSpecDnsConfig<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// The IP addresses of the name servers.
    pub nameservers: Option<Vec<T>>,
    /// A search list for host-name lookup.
    pub search: Option<Vec<T>>,
    /// A list of internal resolver variables to be modified (e.g., `debug`, `ndots:3`, etc.).
    pub options: Option<Vec<T>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecContainerSpecSecrets<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub file: Option<TaskSpecContainerSpecFile<T>>,
    /// SecretID represents the ID of the specific secret that we're referencing.
    #[serde(rename = "SecretID")]
    pub secret_id: Option<T>,
    /// SecretName is the name of the secret that this references, but this is just provided for lookup/display purposes. The secret in the reference will be identified by its ID.
    pub secret_name: Option<T>,
}

/// File represents a specific target that is backed by a file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecContainerSpecFile<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Name represents the final filename in the filesystem.
    pub name: Option<T>,
    /// UID represents the file UID.
    #[serde(rename = "UID")]
    pub uid: Option<T>,
    /// GID represents the file GID.
    #[serde(rename = "GID")]
    pub gid: Option<T>,
    /// Mode represents the FileMode of the file.
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecContainerSpecConfigs<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub file: Option<TaskSpecContainerSpecFile<T>>,
    /// Runtime represents a target that is not mounted into the container but is used by the task  <p><br /><p>  > **Note**: `Configs.File` and `Configs.Runtime` are mutually exclusive
    pub runtime: Option<HashMap<(), ()>>,
    /// ConfigID represents the ID of the specific config that we're referencing.
    #[serde(rename = "ConfigID")]
    pub config_id: Option<T>,
    /// ConfigName is the name of the config that this references, but this is just provided for lookup/display purposes. The config in the reference will be identified by its ID.
    pub config_name: Option<T>,
}

/// Read-only spec type for non-swarm containers attached to swarm overlay networks.  <p><br /></p>  > **Note**: ContainerSpec, NetworkAttachmentSpec, and PluginSpec are > mutually exclusive. PluginSpec is only used when the Runtime field > is set to `plugin`. NetworkAttachmentSpec is used when the Runtime > field is set to `attachment`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecNetworkAttachmentSpec<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// ID of the container represented by this task
    #[serde(rename = "ContainerID")]
    pub container_id: Option<T>,
}

/// Resource requirements which apply to each individual container created as part of the service.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecResources<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Define resources limits.
    pub limits: Option<ResourceObject<T>>,
    /// Define resources reservation.
    pub reservation: Option<ResourceObject<T>>,
}

/// An object describing the resources which can be advertised by a node and requested by a task
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ResourceObject<T>
where
    T: AsRef<str> + Eq + Hash,
{
    #[serde(rename = "NanoCPUs")]
    pub nano_cpus: Option<i64>,
    pub memory_bytes: Option<i64>,
    pub generic_resources: Option<Vec<GenericResources<T>>>,
}

/// User-defined resources can be either Integer resources (e.g, `SSD=3`) or T resources (e.g, `GPU=UUID1`)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct GenericResources<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub named_resource_spec: Option<GenericResourcesNamedResourceSpec<T>>,
    pub discrete_resource_spec: Option<GenericResourcesDiscreteResourceSpec<T>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct GenericResourcesNamedResourceSpec<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub kind: Option<T>,
    pub value: Option<T>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct GenericResourcesDiscreteResourceSpec<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub kind: Option<T>,
    pub value: Option<i64>,
}

/// Specification for the restart policy which applies to containers created as part of this service.
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecRestartPolicy {
    /// Condition for restart.
    pub condition: Option<TaskSpecRestartPolicyCondition>,
    /// Delay between restart attempts.
    pub delay: Option<i64>,
    /// Maximum attempts to restart a given container before giving up (default value is 0, which is ignored).
    pub max_attempts: Option<i64>,
    /// Windows is the time window used to evaluate the restart policy (default value is 0, which is unbounded).
    pub window: Option<i64>,
}

/// Condition for restart.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum TaskSpecRestartPolicyCondition {
    None,
    OnFailure,
    Any,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecPlacement<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// An array of constraints.
    pub constraints: Option<Vec<T>>,
    /// Preferences provide a way to make the scheduler aware of factors such as topology. They are provided in order from highest to lowest precedence.
    pub preferences: Option<Vec<TaskSpecPlacementPreferences<T>>>,
    /// Maximum number of replicas for per node (default value is 0, which is unlimited)
    pub max_replicas: Option<i64>,
    /// Platforms stores all the platforms that the service's image can run on. This field is used in the platform filter for scheduling. If empty, then the platform filter is off, meaning there are no scheduling restrictions.
    pub platforms: Option<Vec<Platform<T>>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecPlacementPreferences<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub spread: Option<TaskSpecPlacementSpread<T>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecPlacementSpread<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// label descriptor, such as engine.labels.az
    pub spread_descriptor: Option<T>,
}

/// Platform represents the platform (Arch/OS).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct Platform<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Architecture represents the hardware architecture (for example, `x86_64`).
    pub architecture: Option<T>,
    /// OS represents the Operating System (for example, `linux` or `windows`).
    #[serde(rename = "OS")]
    pub os: Option<T>,
}

/// Specifies how a service should be attached to a particular network.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct NetworkAttachmentConfig<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// The target network for attachment. Must be a network name or ID.
    pub target: Option<T>,
    /// Discoverable alternate names for the service on this network.
    pub aliases: Option<Vec<T>>,
    /// Driver attachment options for the network target
    pub driver_opts: Option<HashMap<T, String>>,
}

/// Specifies the log driver to use for tasks created from this spec. If not present, the default one for the swarm will be used, finally falling back to the engine default if not specified.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct TaskSpecLogDriver<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub name: Option<T>,
    pub options: Option<HashMap<T, String>>,
}

/// Scheduling mode for the service.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(missing_docs)]
pub enum ServiceSpecMode {
    Replicated {
        #[serde(rename = "Replicas")]
        replicas: i64,
    },
    Global(HashMap<String, String>),
}

/// Specification for the update or rollback strategy of the service.
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ServiceSpecUpdateConfig {
    /// Maximum number of tasks to be updated in one iteration (0 means unlimited parallelism).
    pub parallelism: Option<i64>,
    /// Amount of time between updates, in nanoseconds.
    pub delay: Option<i64>,
    /// Action to take if an updated task fails to run, or stops running during the update.
    pub failure_action: Option<ServiceSpecUpdateConfigFailureAction>,
    /// Amount of time to monitor each updated task for failures, in nanoseconds.
    pub monitor: Option<i64>,
    /// The fraction of tasks that may fail during an update before the failure action is invoked, specified as a floating point number between 0 and 1.
    pub max_failure_ratio: Option<f64>,
    /// The order of operations when rolling out an updated task. Either the old task is shut down before the new task is started, or the new task is started before the old task is shut down.
    pub order: Option<ServiceSpecUpdateConfigOrder>,
}

/// Action to take if an updated task fails to run, or stops running during the update.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum ServiceSpecUpdateConfigFailureAction {
    Continue,
    Pause,
    Rollback,
}

/// The order of operations when rolling out an updated task. Either the old task is shut down before the new task is started, or the new task is started before the old task is shut down.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum ServiceSpecUpdateConfigOrder {
    StopFirst,
    StartFirst,
}

/// Properties that can be configured to access and load balance a service.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct EndpointSpec<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// The mode of resolution to use for internal load balancing between tasks.
    pub mode: Option<EndpointSpecMode>,
    /// List of exposed ports that this service is accessible on from the outside. Ports can only be provided if `vip` resolution mode is used.
    pub ports: Option<Vec<EndpointPortConfig<T>>>,
}

/// The mode of resolution to use for internal load balancing between tasks.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum EndpointSpecMode {
    Vip,
    Dnsrr,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct EndpointPortConfig<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub name: Option<T>,
    pub protocol: Option<EndpointPortConfigProtocol>,
    /// The port inside the container.
    pub target_port: Option<isize>,
    /// The port on the swarm hosts.
    pub published_port: Option<isize>,
    /// The mode in which port is published.  <p><br /></p>  - \"ingress\" makes the target port accessible on every node,   regardless of whether there is a task for the service running on   that node or not. - \"host\" bypasses the routing mesh and publish the port directly on   the swarm node where that service is running.
    pub publish_mode: Option<EndpointPortConfigPublishMode>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum EndpointPortConfigProtocol {
    Tcp,
    Udp,
    Sctp,
}

/// The mode in which port is published.  <p><br /></p>  - \"ingress\" makes the target port accessible on every node,   regardless of whether there is a task for the service running on   that node or not. - \"host\" bypasses the routing mesh and publish the port directly on   the swarm node where that service is running.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum EndpointPortConfigPublishMode {
    Ingress,
    Host,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ServiceEndpoint<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub spec: Option<EndpointSpec<T>>,
    pub ports: Option<Vec<EndpointPortConfig<T>>>,
    #[serde(rename = "VirtualIPs")]
    pub virtual_ips: Option<Vec<ServiceEndpointVirtualIPs<T>>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ServiceEndpointVirtualIPs<T>
where
    T: AsRef<str> + Eq + Hash,
{
    #[serde(rename = "NetworkID")]
    pub network_id: Option<T>,
    pub addr: Option<T>,
}

/// The status of a service update.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ServiceUpdateStatus<T>
where
    T: AsRef<str> + Eq + Hash,
{
    pub state: ServiceUpdateStatusState,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub message: T,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum ServiceUpdateStatusState {
    Updating,
    Paused,
    Completed,
    RollbackCompleted,
}
