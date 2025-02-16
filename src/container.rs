//! Container API: run docker containers and manage their lifecycle
#![allow(deprecated)]

use futures_core::Stream;
use futures_util::{StreamExt, TryStreamExt};
use http::header::{CONNECTION, CONTENT_TYPE, UPGRADE};
use http::request::Builder;
use http_body_util::Full;
use hyper::{body::Bytes, Method};
use serde::Serialize;
use serde_derive::Deserialize;
use tokio::io::AsyncWrite;
use tokio_util::codec::FramedRead;

use std::cmp::Eq;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::pin::Pin;

use super::Docker;
use crate::docker::BodyType;
use crate::errors::Error;
use crate::models::*;
use crate::read::NewlineLogOutputDecoder;

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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::ListContainersOptions and associated ListContainersOptionsBuilder"
)]
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

impl<T> From<ListContainersOptions<T>> for crate::query_parameters::ListContainersOptions
where
    T: Into<String> + Eq + Hash + Serialize,
{
    fn from(opts: ListContainersOptions<T>) -> Self {
        let mut builder = crate::query_parameters::ListContainersOptionsBuilder::default()
            .all(opts.all)
            .size(opts.size)
            .filters(
                &opts
                    .filters
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into_iter().map(T::into).collect()))
                    .collect(),
            );

        if let Some(limit) = opts.limit {
            builder = builder.limit(
                i32::try_from(limit)
                    .inspect_err(|e| {
                        log::error!(
                            "Truncation of isize into i32 in ListContainersOptions: {:?}",
                            e
                        )
                    })
                    .unwrap_or(limit as i32),
            );
        }

        builder.build()
    }
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
///     platform: Some("linux/amd64"),
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::CreateContainerOptions and associated CreateContainerOptionsBuilder"
)]
pub struct CreateContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Assign the specified name to the container.
    pub name: T,

    /// The platform to use for the container.
    /// Added in API v1.41.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<T>,
}

impl<T> From<CreateContainerOptions<T>> for crate::query_parameters::CreateContainerOptions
where
    T: Into<String> + Serialize,
{
    fn from(opts: CreateContainerOptions<T>) -> Self {
        let mut builder = crate::query_parameters::CreateContainerOptionsBuilder::default()
            .name(&opts.name.into());

        if let Some(platform) = opts.platform {
            builder = builder.platform(&platform.into());
        }

        builder.build()
    }
}

/// This container's networking configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct NetworkingConfig<T: Into<String> + Hash + Eq> {
    pub endpoints_config: HashMap<T, EndpointSettings>,
}

impl<T> From<NetworkingConfig<T>> for crate::models::NetworkingConfig
where
    T: Into<String> + Hash + Eq,
{
    fn from(config: NetworkingConfig<T>) -> Self {
        crate::models::NetworkingConfig {
            endpoints_config: Some(
                config
                    .endpoints_config
                    .into_iter()
                    .map(|(k, v)| (k.into(), v))
                    .collect(),
            ),
        }
    }
}

/// Container to create.
/// Note: the swagger codegen is unable to generate this type due to lacking support for `AllOf`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::models::ContainerCreateBody or bollard_stubs::models::ContainerConfig as appropriate"
)]
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

impl<T> From<Config<T>> for ContainerCreateBody
where
    T: Into<String> + Eq + Hash + std::fmt::Debug,
{
    fn from(config: Config<T>) -> Self {
        let mut body = ContainerCreateBody {
            hostname: config.hostname.map(Into::into),
            domainname: config.domainname.map(Into::into),
            user: config.user.map(Into::into),
            attach_stdin: config.attach_stdin,
            attach_stdout: config.attach_stdout,
            attach_stderr: config.attach_stderr,
            exposed_ports: config
                .exposed_ports
                .map(|hsh| hsh.into_iter().map(|(k, v)| (k.into(), v)).collect()),
            tty: config.tty,
            open_stdin: config.open_stdin,
            stdin_once: config.stdin_once,
            env: config.env.map(|v| v.into_iter().map(Into::into).collect()),
            cmd: config.cmd.map(|v| v.into_iter().map(Into::into).collect()),
            healthcheck: config.healthcheck,
            args_escaped: config.args_escaped,
            image: config.image.map(Into::into),
            volumes: config
                .volumes
                .map(|hsh| hsh.into_iter().map(|(k, v)| (k.into(), v)).collect()),
            working_dir: config.working_dir.map(Into::into),
            entrypoint: config
                .entrypoint
                .map(|v| v.into_iter().map(Into::into).collect()),
            network_disabled: config.network_disabled,
            mac_address: config.mac_address.map(Into::into),
            on_build: config
                .on_build
                .map(|v| v.into_iter().map(Into::into).collect()),
            labels: config
                .labels
                .map(|hsh| hsh.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
            stop_signal: config.stop_signal.map(Into::into),
            stop_timeout: config.stop_timeout,
            shell: config
                .shell
                .map(|v| v.into_iter().map(Into::into).collect()),
            ..Default::default()
        };

        body.host_config = config.host_config;
        body.networking_config = config.networking_config.map(Into::into);

        body
    }
}

impl<T> From<Config<T>> for ContainerConfig
where
    T: Into<String> + Eq + Hash + std::fmt::Debug,
{
    fn from(config: Config<T>) -> Self {
        ContainerConfig {
            hostname: config.hostname.map(Into::into),
            domainname: config.domainname.map(Into::into),
            user: config.user.map(Into::into),
            attach_stdin: config.attach_stdin,
            attach_stdout: config.attach_stdout,
            attach_stderr: config.attach_stderr,
            exposed_ports: config
                .exposed_ports
                .map(|hsh| hsh.into_iter().map(|(k, v)| (k.into(), v)).collect()),
            tty: config.tty,
            open_stdin: config.open_stdin,
            stdin_once: config.stdin_once,
            env: config.env.map(|v| v.into_iter().map(Into::into).collect()),
            cmd: config.cmd.map(|v| v.into_iter().map(Into::into).collect()),
            healthcheck: config.healthcheck,
            args_escaped: config.args_escaped,
            image: config.image.map(Into::into),
            volumes: config
                .volumes
                .map(|hsh| hsh.into_iter().map(|(k, v)| (k.into(), v)).collect()),
            working_dir: config.working_dir.map(Into::into),
            entrypoint: config
                .entrypoint
                .map(|v| v.into_iter().map(Into::into).collect()),
            network_disabled: config.network_disabled,
            mac_address: config.mac_address.map(Into::into),
            on_build: config
                .on_build
                .map(|v| v.into_iter().map(Into::into).collect()),
            labels: config
                .labels
                .map(|hsh| hsh.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
            stop_signal: config.stop_signal.map(Into::into),
            stop_timeout: config.stop_timeout,
            shell: config
                .shell
                .map(|v| v.into_iter().map(Into::into).collect()),
        }
    }
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
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::StopContainerOptions and associated StopContainerOptionsBuilder"
)]
pub struct StopContainerOptions {
    /// Number of seconds to wait before killing the container
    pub t: i64,
}

impl From<StopContainerOptions> for crate::query_parameters::StopContainerOptions {
    fn from(opts: StopContainerOptions) -> Self {
        crate::query_parameters::StopContainerOptionsBuilder::default()
            .t(i32::try_from(opts.t)
                .inspect_err(|e| {
                    log::error!(
                        "Truncation of i64 into i32 in StopContainerOptions: {:?}",
                        e
                    )
                })
                .unwrap_or(opts.t as i32))
            .build()
    }
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::StartContainerOptions and associated StartContainerOptionsBuilder"
)]
#[serde(rename_all = "camelCase")]
pub struct StartContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Override the key sequence for detaching a container. Format is a single character `[a-Z]` or
    /// `ctrl-<value>` where `<value>` is one of: `a-z`, `@`, `^`, `[`, `,` or `_`.
    pub detach_keys: T,
}

impl<T> From<StartContainerOptions<T>> for crate::query_parameters::StartContainerOptions
where
    T: Into<String> + Serialize,
{
    fn from(opts: StartContainerOptions<T>) -> Self {
        crate::query_parameters::StartContainerOptionsBuilder::default()
            .detach_keys(&opts.detach_keys.into())
            .build()
    }
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
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::RemoveContainerOptions and associated RemoveContainerOptionsBuilder"
)]
pub struct RemoveContainerOptions {
    /// Remove the volumes associated with the container.
    pub v: bool,
    /// If the container is running, kill it before removing it.
    pub force: bool,
    /// Remove the specified link associated with the container.
    pub link: bool,
}

impl From<RemoveContainerOptions> for crate::query_parameters::RemoveContainerOptions {
    fn from(opts: RemoveContainerOptions) -> Self {
        crate::query_parameters::RemoveContainerOptionsBuilder::default()
            .v(opts.v)
            .force(opts.force)
            .link(opts.link)
            .build()
    }
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::WaitContainerOptions and associated WaitContainerOptionsBuilder"
)]
pub struct WaitContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Wait until a container state reaches the given condition, either 'not-running' (default),
    /// 'next-exit', or 'removed'.
    pub condition: T,
}

impl<T> From<WaitContainerOptions<T>> for crate::query_parameters::WaitContainerOptions
where
    T: Into<String> + Serialize,
{
    fn from(opts: WaitContainerOptions<T>) -> Self {
        crate::query_parameters::WaitContainerOptionsBuilder::default()
            .condition(&opts.condition.into())
            .build()
    }
}

/// Results type for the [Attach Container API](Docker::attach_container())
pub struct AttachContainerResults {
    /// [Log Output](LogOutput) enum, wrapped in a Stream.
    pub output: Pin<Box<dyn Stream<Item = Result<LogOutput, Error>> + Send>>,
    /// Byte writer to container
    pub input: Pin<Box<dyn AsyncWrite + Send>>,
}

impl fmt::Debug for AttachContainerResults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AttachContainerResults")
    }
}

/// Parameters used in the [Attach Container API](Docker::attach_container())
///
/// ## Examples
///
/// ```rust
/// use bollard::container::AttachContainerOptions;
///
/// AttachContainerOptions::<String>{
///     stdin: Some(true),
///     stdout: Some(true),
///     stderr: Some(true),
///     stream: Some(true),
///     logs: Some(true),
///     detach_keys: Some("ctrl-c".to_string()),
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::AttachContainerOptions and associated AttachContainerOptionsBuilder"
)]
pub struct AttachContainerOptions<T>
where
    T: Into<String> + Serialize + Default,
{
    /// Attach to `stdin`
    pub stdin: Option<bool>,
    /// Attach to `stdout`
    pub stdout: Option<bool>,
    /// Attach to `stderr`
    pub stderr: Option<bool>,
    /// Stream attached streams from the time the request was made onwards.
    pub stream: Option<bool>,
    /// Replay previous logs from the container.
    /// This is useful for attaching to a container that has started and you want to output everything since the container started.
    /// If stream is also enabled, once all the previous output has been returned, it will seamlessly transition into streaming current output.
    pub logs: Option<bool>,
    /// Override the key sequence for detaching a container.
    /// Format is a single character [a-Z] or ctrl-\<value\> where \<value\> is one of: a-z, @, ^, [, , or _.
    #[serde(rename = "detachKeys")]
    pub detach_keys: Option<T>,
}

impl<T> From<AttachContainerOptions<T>> for crate::query_parameters::AttachContainerOptions
where
    T: Into<String> + Serialize + Default,
{
    fn from(opts: AttachContainerOptions<T>) -> Self {
        let mut builder = crate::query_parameters::AttachContainerOptionsBuilder::default();
        if let Some(stdin) = opts.stdin {
            builder = builder.stdin(stdin);
        }
        if let Some(stdout) = opts.stdout {
            builder = builder.stdout(stdout);
        }
        if let Some(stderr) = opts.stderr {
            builder = builder.stderr(stderr);
        }
        if let Some(stream) = opts.stream {
            builder = builder.stream(stream);
        }
        if let Some(logs) = opts.logs {
            builder = builder.logs(logs);
        }
        if let Some(detach_keys) = opts.detach_keys {
            builder = builder.detach_keys(&detach_keys.into());
        }
        builder.build()
    }
}

/// Parameters used in the [Resize Container Tty API](Docker::resize_container_tty())
///
/// ## Examples
///
/// ```rust
/// use bollard::container::ResizeContainerTtyOptions;
///
/// ResizeContainerTtyOptions {
///     width: 50,
///     height: 10,
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::ResizeContainerTTYOptions and associated ResizeContainerTTYOptionsBuilder"
)]
pub struct ResizeContainerTtyOptions {
    /// Width of the TTY session in characters
    #[serde(rename = "w")]
    pub width: u16,
    /// Height of the TTY session in characters
    #[serde(rename = "h")]
    pub height: u16,
}

impl From<ResizeContainerTtyOptions> for crate::query_parameters::ResizeContainerTTYOptions {
    fn from(opts: ResizeContainerTtyOptions) -> Self {
        crate::query_parameters::ResizeContainerTTYOptionsBuilder::default()
            .w(opts.width as i32)
            .h(opts.height as i32)
            .build()
    }
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
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::RestartContainerOptions and associated RestartContainerOptionsBuilder"
)]
pub struct RestartContainerOptions {
    /// Number of seconds to wait before killing the container.
    pub t: isize,
}

impl From<RestartContainerOptions> for crate::query_parameters::RestartContainerOptions {
    fn from(opts: RestartContainerOptions) -> Self {
        crate::query_parameters::RestartContainerOptionsBuilder::default()
            .t(i32::try_from(opts.t)
                .inspect_err(|e| {
                    log::error!(
                        "Truncation of isize into i32 in RestartContainerOptions : {:?}",
                        e
                    )
                })
                .unwrap_or(opts.t as i32))
            .build()
    }
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
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::InspectContainerOptions and associated InspectContainerOptionsBuilder"
)]
pub struct InspectContainerOptions {
    /// Return the size of container as fields `SizeRw` and `SizeRootFs`
    pub size: bool,
}

impl From<InspectContainerOptions> for crate::query_parameters::InspectContainerOptions {
    fn from(opts: InspectContainerOptions) -> Self {
        crate::query_parameters::InspectContainerOptionsBuilder::default()
            .size(opts.size)
            .build()
    }
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::TopContainerOptions and associated TopContainerOptionsBuilder"
)]
pub struct TopOptions<T>
where
    T: Into<String> + Serialize,
{
    /// The arguments to pass to `ps`. For example, `aux`
    pub ps_args: T,
}

impl<T> From<TopOptions<T>> for crate::query_parameters::TopOptions
where
    T: Into<String> + Serialize,
{
    fn from(opts: TopOptions<T>) -> Self {
        crate::query_parameters::TopOptionsBuilder::default()
            .ps_args(&opts.ps_args.into())
            .build()
    }
}

fn is_zero(val: &i64) -> bool {
    val == &0i64
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::LogsOptions and associated LogsOptionsBuilder"
)]
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
    #[serde(skip_serializing_if = "is_zero")]
    // workaround for https://github.com/containers/podman/issues/10859
    pub until: i64,
    /// Add timestamps to every log line.
    pub timestamps: bool,
    /// Only return this number of log lines from the end of the logs. Specify as an integer or all
    /// to output `all` log lines.
    pub tail: T,
}

impl<T> From<LogsOptions<T>> for crate::query_parameters::LogsOptions
where
    T: Into<String> + Serialize,
{
    fn from(opts: LogsOptions<T>) -> Self {
        crate::query_parameters::LogsOptionsBuilder::default()
            .follow(opts.follow)
            .stdout(opts.stdout)
            .stderr(opts.stderr)
            .since(
                i32::try_from(opts.since)
                    .inspect_err(|e| {
                        log::error!("Truncation of i64 into i32 in LogsOptions : {:?}", e)
                    })
                    .unwrap_or(opts.since as i32),
            )
            .until(
                i32::try_from(opts.until)
                    .inspect_err(|e| {
                        log::error!("Truncation of i64 into i32 in LogsOptions : {:?}", e)
                    })
                    .unwrap_or(opts.until as i32),
            )
            .timestamps(opts.timestamps)
            .tail(&opts.tail.into())
            .build()
    }
}

/// Result type for the [Logs API](Docker::logs())
#[derive(Debug, Clone, PartialEq)]
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
        write!(f, "{}", String::from_utf8_lossy(message))
    }
}

impl AsRef<[u8]> for LogOutput {
    fn as_ref(&self) -> &[u8] {
        match self {
            LogOutput::StdErr { message } => message.as_ref(),
            LogOutput::StdOut { message } => message.as_ref(),
            LogOutput::StdIn { message } => message.as_ref(),
            LogOutput::Console { message } => message.as_ref(),
        }
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
///     one_shot: false,
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::StatsOptions and associated StatsOptionsBuilder"
)]
pub struct StatsOptions {
    /// Stream the output. If false, the stats will be output once and then it will disconnect.
    pub stream: bool,
    /// Only get a single stat instead of waiting for 2 cycles. Must be used with `stream = false`.
    #[serde(rename = "one-shot")]
    pub one_shot: bool,
}

impl From<StatsOptions> for crate::query_parameters::StatsOptions {
    fn from(opts: StatsOptions) -> Self {
        crate::query_parameters::StatsOptionsBuilder::default()
            .stream(opts.stream)
            .one_shot(opts.one_shot)
            .build()
    }
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::KillContainerOptions and associated KillContainerOptionsBuilder"
)]
pub struct KillContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Signal to send to the container as an integer or string (e.g. `SIGINT`)
    pub signal: T,
}

impl<T> From<KillContainerOptions<T>> for crate::query_parameters::KillContainerOptions
where
    T: Into<String> + Serialize,
{
    fn from(opts: KillContainerOptions<T>) -> Self {
        crate::query_parameters::KillContainerOptionsBuilder::default()
            .signal(&opts.signal.into())
            .build()
    }
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::UpdateContainerOptions and associated UpdateContainerOptionsBuilder"
)]
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
    #[serde(rename = "NanoCpus")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nano_cpus: Option<i64>,

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
    pub io_maximum_iops: Option<i64>,

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

impl<T> From<UpdateContainerOptions<T>> for ContainerUpdateBody
where
    T: Into<String> + Eq + Hash,
{
    fn from(opts: UpdateContainerOptions<T>) -> Self {
        let mut container_update = ContainerUpdateBody {
            cpu_shares: opts.cpu_shares.map(|x| x as i64),
            memory: opts.memory,
            cgroup_parent: opts.cgroup_parent.map(T::into),
            blkio_weight: opts.blkio_weight,
            blkio_weight_device: opts.blkio_weight_device,
            blkio_device_read_bps: opts.blkio_device_read_bps,
            blkio_device_write_bps: opts.blkio_device_write_bps,
            blkio_device_read_iops: opts.blkio_device_read_i_ops,
            blkio_device_write_iops: opts.blkio_device_write_i_ops,
            cpu_period: opts.cpu_period,
            cpu_quota: opts.cpu_quota,
            cpu_realtime_period: opts.cpu_realtime_period,
            cpu_realtime_runtime: opts.cpu_realtime_runtime,
            cpuset_cpus: opts.cpuset_cpus.map(T::into),
            cpuset_mems: opts.cpuset_mems.map(T::into),
            devices: opts.devices,
            device_cgroup_rules: opts
                .device_cgroup_rules
                .map(|v| v.into_iter().map(T::into).collect()),
            device_requests: opts.device_requests,
            kernel_memory_tcp: opts.kernel_memory_tcp,
            memory_reservation: opts.memory_reservation,
            memory_swap: opts.memory_swap,
            memory_swappiness: opts.memory_swappiness,
            nano_cpus: opts.nano_cpus,
            oom_kill_disable: opts.oom_kill_disable,
            init: opts.init,
            pids_limit: opts.pids_limit,
            ulimits: opts.ulimits,
            cpu_count: opts.cpu_count,
            cpu_percent: opts.cpu_percent,
            io_maximum_iops: opts.io_maximum_iops,
            io_maximum_bandwidth: opts.io_maximum_bandwidth,
            ..Default::default()
        };

        container_update.restart_policy = opts.restart_policy;

        container_update
    }
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::RenameContainerOptions and associated RenameContainerOptionsBuilder"
)]
pub struct RenameContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// New name for the container.
    pub name: T,
}

impl<T> From<RenameContainerOptions<T>> for crate::query_parameters::RenameContainerOptions
where
    T: Into<String> + Serialize,
{
    fn from(opts: RenameContainerOptions<T>) -> Self {
        crate::query_parameters::RenameContainerOptionsBuilder::default()
            .name(&opts.name.into())
            .build()
    }
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::PruneContainerOptions and associated PruneContainerOptionsBuilder"
)]
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

impl<T> From<PruneContainersOptions<T>> for crate::query_parameters::PruneContainersOptions
where
    T: Into<String> + Eq + Hash + Serialize,
{
    fn from(opts: PruneContainersOptions<T>) -> Self {
        crate::query_parameters::PruneContainersOptionsBuilder::default()
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::UploadToContainerOptions and associated UploadToContainerOptionsBuilder"
)]
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

impl<T> From<UploadToContainerOptions<T>> for crate::query_parameters::UploadToContainerOptions
where
    T: Into<String> + Serialize,
{
    fn from(opts: UploadToContainerOptions<T>) -> Self {
        crate::query_parameters::UploadToContainerOptionsBuilder::default()
            .path(&opts.path.into())
            .no_overwrite_dir_non_dir(&opts.no_overwrite_dir_non_dir.into())
            .build()
    }
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::DownloadFromContainerOptions and associated DownloadFromContainerOptionsBuilder"
)]
pub struct DownloadFromContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Resource in the container’s filesystem to archive.
    pub path: T,
}

impl<T> From<DownloadFromContainerOptions<T>>
    for crate::query_parameters::DownloadFromContainerOptions
where
    T: Into<String> + Serialize,
{
    fn from(opts: DownloadFromContainerOptions<T>) -> Self {
        crate::query_parameters::DownloadFromContainerOptionsBuilder::default()
            .path(&opts.path.into())
            .build()
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
    ///  - Optional [ListContainersOptions](ListContainersOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [ContainerSummary](ContainerSummary), wrapped in a Future.
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
    pub async fn list_containers(
        &self,
        options: Option<impl Into<crate::query_parameters::ListContainersOptions>>,
    ) -> Result<Vec<ContainerSummary>, Error> {
        let url = "/containers/json";

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
    ///     platform: None,
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
    pub async fn create_container(
        &self,
        options: Option<impl Into<crate::query_parameters::CreateContainerOptions>>,
        config: impl Into<ContainerCreateBody>,
    ) -> Result<ContainerCreateResponse, Error> {
        let url = "/containers/create";
        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            options.map(Into::into),
            Docker::serialize_payload(Some(config.into())),
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
    pub async fn start_container(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::StartContainerOptions>>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/start");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
        options: Option<impl Into<StopContainerOptions>>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/stop");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
        options: Option<impl Into<crate::query_parameters::RemoveContainerOptions>>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
    ///    Stream.
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
    pub fn wait_container(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::WaitContainerOptions>>,
    ) -> impl Stream<Item = Result<ContainerWaitResponse, Error>> {
        let url = format!("/containers/{container_name}/wait");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_stream(req).map(|res| match res {
            Ok(ContainerWaitResponse {
                status_code: code,
                error:
                    Some(ContainerWaitExitError {
                        message: Some(error),
                    }),
            }) if code > 0 => Err(Error::DockerContainerWaitError { error, code }),
            Ok(ContainerWaitResponse {
                status_code: code,
                error: None,
            }) if code > 0 => Err(Error::DockerContainerWaitError {
                error: String::new(),
                code,
            }),
            v => v,
        })
    }

    /// ---
    ///
    /// # Attach Container
    ///
    /// Attach to a container to read its output or send it input. You can attach to the
    /// same container multiple times and you can reattach to containers that have been detached.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - Optional [Attach Container Options](AttachContainerOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [AttachContainerResults](AttachContainerResults) wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::container::AttachContainerOptions;
    ///
    /// let options = Some(AttachContainerOptions::<String>{
    ///     stdin: Some(true),
    ///     stdout: Some(true),
    ///     stderr: Some(true),
    ///     stream: Some(true),
    ///     logs: Some(true),
    ///     detach_keys: Some("ctrl-c".to_string()),
    /// });
    ///
    /// docker.attach_container("hello-world", options);
    /// ```
    pub async fn attach_container(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::AttachContainerOptions>>,
    ) -> Result<AttachContainerResults, Error> {
        let url = format!("/containers/{container_name}/attach");

        let req = self.build_request(
            &url,
            Builder::new()
                .method(Method::POST)
                .header(CONNECTION, "Upgrade")
                .header(UPGRADE, "tcp"),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        let (read, write) = self.process_upgraded(req).await?;
        let log = FramedRead::new(read, NewlineLogOutputDecoder::new(true)).map_err(|e| e.into());

        Ok(AttachContainerResults {
            output: Box::pin(log),
            input: Box::pin(write),
        })
    }

    /// ---
    ///
    /// # Resize container tty
    ///
    /// Resize the container's TTY.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - [Resize Container Tty Options](ResizeContainerTtyOptions) struct.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::container::ResizeContainerTtyOptions;
    ///
    /// let options = ResizeContainerTtyOptions {
    ///     width: 50,
    ///     height: 20,
    /// };
    ///
    /// docker.resize_container_tty("hello-world", options);
    /// ```
    pub async fn resize_container_tty(
        &self,
        container_name: &str,
        options: impl Into<crate::query_parameters::ResizeContainerTTYOptions>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/resize");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Some(options.into()),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
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
        options: Option<impl Into<RestartContainerOptions>>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/restart");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
        options: Option<impl Into<crate::query_parameters::InspectContainerOptions>>,
    ) -> Result<ContainerInspectResponse, Error> {
        let url = format!("/containers/{container_name}/json");

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
    pub async fn top_processes(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::TopOptions>>,
    ) -> Result<ContainerTopResponse, Error> {
        let url = format!("/containers/{container_name}/top");

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
    ///    Stream.
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
    pub fn logs(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::LogsOptions>>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> {
        let url = format!("/containers/{container_name}/logs");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
    ///  - An Option of Vector of [File System Change](FilesystemChange) structs, wrapped in a
    ///    Future.
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
    ) -> Result<Option<Vec<FilesystemChange>>, Error> {
        let url = format!("/containers/{container_name}/changes");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
    ///    Stream.
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
    ///     one_shot: true,
    /// });
    ///
    /// docker.stats("hello-world", options);
    /// ```
    pub fn stats(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::StatsOptions>>,
    ) -> impl Stream<Item = Result<ContainerStatsResponse, Error>> {
        let url = format!("/containers/{container_name}/stats");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
    pub async fn kill_container(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::KillContainerOptions>>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/kill");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
    pub async fn update_container(
        &self,
        container_name: &str,
        config: impl Into<ContainerUpdateBody>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/update");

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
    pub async fn rename_container(
        &self,
        container_name: &str,
        options: impl Into<crate::query_parameters::RenameContainerOptions>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/rename");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Some(options.into()),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
        let url = format!("/containers/{container_name}/pause");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
        let url = format!("/containers/{container_name}/unpause");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
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
    pub async fn prune_containers(
        &self,
        options: Option<impl Into<crate::query_parameters::PruneContainersOptions>>,
    ) -> Result<ContainerPruneResponse, Error> {
        let url = "/containers/prune";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Stream Upload To Container
    ///
    /// Stream an upload of a tar archive to be extracted to a path in the filesystem of container
    /// id.
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
    /// use bollard::container::UploadToContainerOptions;
    /// use futures_util::{StreamExt, TryFutureExt};
    /// use tokio::fs::File;
    /// use tokio_util::io::ReaderStream;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let options = Some(UploadToContainerOptions{
    ///     path: "/opt",
    ///     ..Default::default()
    /// });
    ///
    /// let file = File::open("tarball.tar.gz")
    ///     .map_ok(ReaderStream::new)
    ///     .try_flatten_stream()
    ///     .map(|x|x.expect("failed to stream file"));
    ///
    /// docker
    ///     .upload_to_container_streaming("my-container", options, file)
    ///     .await
    ///     .expect("upload failed");
    /// # }
    /// ```
    #[inline(always)]
    #[deprecated(
        since = "0.19.0",
        note = "This method is refactored into upload_to_container"
    )]
    pub async fn upload_to_container_streaming(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::UploadToContainerOptions>>,
        tar: impl Stream<Item = Bytes> + Send + 'static,
    ) -> Result<(), Error> {
        self.upload_to_container(
            container_name,
            options.map(Into::into),
            crate::body_stream(tar),
        )
        .await
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
    /// Uploading a tarball
    ///
    /// ```rust,no_run
    /// # use bollard::Docker;
    /// use bollard::container::UploadToContainerOptions;
    /// use bollard::body_full;
    /// use std::fs::File;
    /// use std::io::Read;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let options = Some(UploadToContainerOptions{
    ///     path: "/opt",
    ///     ..Default::default()
    /// });
    ///
    /// let mut file = File::open("tarball.tar.gz").unwrap();
    /// let mut contents = Vec::new();
    /// file.read_to_end(&mut contents).unwrap();
    ///
    /// docker
    ///     .upload_to_container("my-container", options, body_full(contents.into()))
    ///     .await
    ///     .expect("upload failed");
    /// # }
    /// ```
    /// Uploading a stream
    ///
    /// ```rust,no_run
    /// # use bollard::Docker;
    /// use bollard::container::UploadToContainerOptions;
    /// use bollard::body_try_stream;
    /// use futures_util::{StreamExt, TryFutureExt};
    /// use tokio::fs::File;
    /// use tokio_util::io::ReaderStream;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let options = Some(UploadToContainerOptions{
    ///     path: "/opt",
    ///     ..Default::default()
    /// });
    ///
    /// let file = File::open("tarball.tar.gz")
    ///     .map_ok(ReaderStream::new)
    ///     .try_flatten_stream();
    ///
    /// docker
    ///     .upload_to_container("my-container", options, body_try_stream(file))
    ///     .await
    ///     .expect("upload failed");
    /// # }
    /// ```
    pub async fn upload_to_container(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::UploadToContainerOptions>>,
        tar: BodyType,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/archive");

        let req = self.build_request(
            &url,
            Builder::new()
                .method(Method::PUT)
                .header(CONTENT_TYPE, "application/x-tar"),
            options.map(Into::into),
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
    pub fn download_from_container(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::DownloadFromContainerOptions>>,
    ) -> impl Stream<Item = Result<Bytes, Error>> {
        let url = format!("/containers/{container_name}/archive");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_body(req)
    }

    /// ---
    ///
    /// # Export Container
    ///
    /// Get a tarball containing the filesystem contents of a container.
    ///
    /// See the [Docker API documentation](https://docs.docker.com/engine/api/v1.40/#operation/ContainerExport)
    /// for more information.
    /// # Arguments
    /// - The `container_name` string referring to an individual container
    ///
    /// # Returns
    ///  - An uncompressed TAR archive
    pub fn export_container(
        &self,
        container_name: &str,
    ) -> impl Stream<Item = Result<Bytes, Error>> {
        let url = format!("/containers/{container_name}/export");
        let req = self.build_request(
            &url,
            Builder::new()
                .method(Method::GET)
                .header(CONTENT_TYPE, "application/json"),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );
        self.process_into_body(req)
    }
}

#[cfg(not(windows))]
#[cfg(test)]
mod tests {

    use futures_util::TryStreamExt;
    use yup_hyper_mock::HostToReplyConnector;

    use crate::{Docker, API_DEFAULT_VERSION};

    use super::WaitContainerOptions;

    #[tokio::test]
    async fn test_container_wait_with_error() {
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            String::from("http://127.0.0.1"),
            "HTTP/1.1 200 OK\r\nServer:mock1\r\nContent-Type:application/json\r\n\r\n{\"Error\":null,\"StatusCode\":1}".to_string(),
        );

        let docker =
            Docker::connect_with_mock(connector, "127.0.0.1".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let result = &docker
            .wait_container("wait_container_test", None::<WaitContainerOptions<String>>)
            .try_collect::<Vec<_>>()
            .await;

        assert!(matches!(
            result,
            Err(crate::errors::Error::DockerContainerWaitError { code: _, error: _ })
        ));
    }

    #[tokio::test]
    async fn test_output_non_json_error() {
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            String::from("http://127.0.0.1"),
            "HTTP/1.1 200 OK\r\nServer:mock1\r\nContent-Type:plain/text\r\n\r\nthis is not json"
                .to_string(),
        );
        let docker =
            Docker::connect_with_mock(connector, "127.0.0.1".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let host_config = bollard_stubs::models::HostConfig {
            mounts: Some(vec![bollard_stubs::models::Mount {
                target: Some(String::from("/tmp")),
                source: Some(String::from("./tmp")),
                typ: Some(bollard_stubs::models::MountTypeEnum::BIND),
                consistency: Some(String::from("default")),
                ..Default::default()
            }]),
            ..Default::default()
        };

        let result = &docker
            .create_container(
                Some(crate::container::CreateContainerOptions {
                    name: "mount_volume_container_failure_test",
                    platform: None,
                }),
                crate::container::Config {
                    image: Some("some_image"),
                    host_config: Some(host_config),
                    ..Default::default()
                },
            )
            .await;

        println!("{result:#?}");

        assert!(matches!(
            result,
            Err(crate::errors::Error::JsonDataError {
                message: _,
                column: 2
            })
        ));
    }
}
