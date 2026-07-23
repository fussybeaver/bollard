#![cfg(feature = "buildkit_providerless")]

use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};

use bollard_buildkit_proto::moby::buildkit::v1::control_client::ControlClient;
use bollard_stubs::models::{
    ContainerCreateBody, ContainerInspectResponse, ContainerStateStatusEnum, ExecInspectResponse,
    HostConfig, Mount, MountType, RestartPolicy, RestartPolicyNameEnum, SystemInfoCgroupDriverEnum,
    Volume, VolumeCreateRequest,
};
use bytes::BytesMut;
use futures_core::Future;
use futures_util::{StreamExt, TryFutureExt, TryStreamExt};
use http::{
    header::{CONNECTION, UPGRADE},
    request::Builder,
    Method,
};
use log::{debug, info, warn};
use tonic::transport::Endpoint;
use tonic::{codegen::InterceptedService, transport::Channel};
use tower_service::Service;

use crate::{
    auth::DockerCredentials,
    exec::{CreateExecOptions, StartExecOptions, StartExecResults},
    grpc::{
        build::{ImageBuildFrontendOptions, ImageBuildLoadInput},
        error::GrpcError,
        io::GrpcFramedTransport,
        registry::ImageRegistryOutput,
        BuildRef, GrpcServer,
    },
    Docker,
};

use super::{channel::BuildkitChannel, DriverInterceptor, ImageExporterEnum};

/// The default `Buildkit` image to use for the [`DockerContainer] driver.
pub const DEFAULT_IMAGE: &str = "moby/buildkit:master";
const DEFAULT_STATE_DIR: &str = "/var/lib/buildkit";
const DUPLEX_BUF_SIZE: usize = 8 * 1024;
const LABEL_MANAGED: &str = "com.github.fussybeaver.bollard.buildkit.managed";
const LABEL_DRIVER: &str = "com.github.fussybeaver.bollard.buildkit.driver";
const LABEL_LIFECYCLE_SCHEMA: &str = "com.github.fussybeaver.bollard.buildkit.lifecycle-schema";
const LABEL_BUILDER_NAME: &str = "com.github.fussybeaver.bollard.buildkit.builder-name";
const LABEL_STATE_VOLUME: &str = "com.github.fussybeaver.bollard.buildkit.state-volume";
const LABEL_RESOURCE_ID: &str = "com.github.fussybeaver.bollard.buildkit.resource-id";
const LIFECYCLE_SCHEMA_VERSION: &str = "1";
const DOCKER_STOP_GRACE_PERIOD: u64 = 10;
const DOCKER_STOP_TIMEOUT: Duration = Duration::from_secs(15);

/// Controls the lifetime of a Docker container provisioned for BuildKit.
///
/// [`DockerContainerLifecycle::Persistent`] is the default. Persistent builders
/// remain available until callers explicitly invoke [`DockerContainer::stop`]
/// or [`DockerContainer::remove`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DockerContainerLifecycle {
    /// Keep the BuildKit container and its state volume after a solve.
    #[default]
    Persistent,
    /// Stop the BuildKit container after a solve while retaining its resources.
    StopAfterSolve,
    /// Remove the BuildKit container after a solve, optionally retaining state.
    RemoveAfterSolve {
        /// Keep the state volume when removing the container.
        keep_state: bool,
    },
}

/// Options for explicitly removing a Docker-container BuildKit builder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DockerContainerRemoveOptions {
    keep_state: bool,
}

impl DockerContainerRemoveOptions {
    /// Construct options that remove the builder and its state volume.
    pub fn new() -> Self {
        Self::default()
    }

    /// Keep the state volume after removing the BuildKit container.
    pub fn keep_state(mut self, keep_state: bool) -> Self {
        self.keep_state = keep_state;
        self
    }
}

impl Service<tonic::transport::Uri> for DockerContainer {
    type Response = GrpcFramedTransport;
    type Error = GrpcError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: tonic::transport::Uri) -> Self::Future {
        DockerContainerConnector {
            client: Docker::clone(&self.docker),
            name: String::clone(&self.name),
        }
        .call(_req)
    }
}

#[derive(Debug, Clone)]
struct DockerContainerConnector {
    client: Docker,
    name: String,
}

impl Service<tonic::transport::Uri> for DockerContainerConnector {
    type Response = GrpcFramedTransport;
    type Error = GrpcError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: tonic::transport::Uri) -> Self::Future {
        let client = Docker::clone(&self.client);
        let name = String::clone(&self.name);

        let fut = async move {
            let exec_id = client
                .create_exec(
                    &name,
                    CreateExecOptions {
                        attach_stdin: Some(true),
                        attach_stdout: Some(true),
                        attach_stderr: Some(true),
                        cmd: Some(vec!["buildctl", "dial-stdio"]),
                        ..Default::default()
                    },
                )
                .await?
                .id;

            let url = format!("/exec/{exec_id}/start");
            let capacity = 8 * 1024;

            let req = client.build_request(
                &url,
                Builder::new()
                    .method(Method::POST)
                    .header(CONNECTION, "Upgrade")
                    .header(UPGRADE, "tcp"),
                None::<String>,
                Docker::serialize_payload(Some(StartExecOptions {
                    output_capacity: Some(capacity),
                    ..Default::default()
                })),
            );

            client.process_upgraded(req).await.map(|(read, write)| {
                let output = Box::pin(read);
                let input = Box::pin(write);
                GrpcFramedTransport::new(output, input, capacity)
            })
        };

        Box::pin(fut.map_err(From::from))
    }
}

/// Builder used to create a driver, needed to communicate with `Buildkit`, such as with the
/// [`crate::grpc::driver::Export::export`] or [`crate::grpc::driver::Image::registry`]
/// functionality.
///
/// <div class="warning">
///  Warning: Buildkit features in Bollard are currently in Developer Preview and are intended strictly for feedback purposes only.
/// </div>
///
/// ## Examples
///
/// ```rust,no_run
/// use bollard::grpc::driver::docker_container::DockerContainerBuilder;
/// use bollard::Docker;
///
/// // Use a connection function
/// // let docker = Docker::connect_...;
/// # let docker = Docker::connect_with_local_defaults().unwrap();
///
/// let builder = DockerContainerBuilder::new(&docker);
///
/// ```
///
#[derive(Debug)]
pub struct DockerContainerBuilder {
    inner: DockerContainer,
}

impl DockerContainerBuilder {
    /// Construct a new `DockerContainerBuilder` to build a [`DockerContainer`].
    /// The Docker container remains available after solving a build by default.
    ///
    /// # Arguments
    ///
    ///  - A reference to the docker client
    pub fn new(docker: &Docker) -> Self {
        Self {
            inner: DockerContainer {
                name: format!("bollard_buildkit_{}", crate::grpc::new_id()),
                docker: Docker::clone(docker),
                net_mode: None,
                image: None,
                cgroup_parent: None,
                env: vec![],
                args: vec![],
                lifecycle: DockerContainerLifecycle::Persistent,
                resource_id: crate::grpc::new_id(),
            },
        }
    }

    /// Consume this builder to construct a [`DockerContainer`].
    pub async fn bootstrap(mut self) -> Result<DockerContainer, GrpcError> {
        debug!("booting buildkit");

        validate_container_name(&self.inner.name)?;

        if self.inner.net_mode.is_none() {
            self.network("host");
        }

        let cleanup_state = Arc::new(Mutex::new(BootstrapCleanupState {
            enabled: false,
            container_created: false,
            volume_created: false,
            name: String::from(&self.inner.name),
            volume_name: self.inner.state_volume_name(),
            resource_id: String::clone(&self.inner.resource_id),
        }));
        let tear_down_handler = Box::new(BootstrapTearDownHandler {
            docker: Docker::clone(&self.inner.docker),
            state: Arc::clone(&cleanup_state),
        });
        let mut tear_down_guard = super::TearDownGuard::new(tear_down_handler);

        let host_config = self.inner.desired_host_config().await?;
        let existing_container = inspect_container(&self.inner.docker, &self.inner.name).await?;
        let existing_volume =
            inspect_volume(&self.inner.docker, &self.inner.state_volume_name()).await?;

        match (existing_container, existing_volume) {
            (Some(container), Some(volume)) => {
                let resource_id = compatible_volume(&self.inner, &volume)?;
                compatible_container(&self.inner, &container, &host_config, &resource_id)?;
                self.inner.resource_id = resource_id;

                match container.state.and_then(|state| state.status) {
                    Some(ContainerStateStatusEnum::RUNNING)
                    | Some(ContainerStateStatusEnum::RESTARTING) => {}
                    Some(ContainerStateStatusEnum::CREATED)
                    | Some(ContainerStateStatusEnum::EXITED) => self.inner.start().await?,
                    Some(status) => {
                        return Err(resource_conflict(
                            "container",
                            &self.inner.name,
                            format!("container is in unsupported state `{status}`"),
                        ));
                    }
                    None => {
                        return Err(resource_conflict(
                            "container",
                            &self.inner.name,
                            "container state is unavailable",
                        ));
                    }
                }
            }
            (None, Some(volume)) => {
                self.inner.resource_id = compatible_volume(&self.inner, &volume)?;
                update_cleanup_state(&cleanup_state, |state| {
                    state.enabled = true;
                    state.container_created = true;
                    state.resource_id = String::clone(&self.inner.resource_id);
                });
                self.inner.create(&host_config).await?;
            }
            (Some(container), None) => {
                if let Some(labels) = container
                    .config
                    .as_ref()
                    .and_then(|config| config.labels.as_ref())
                {
                    if let Err(reason) =
                        compatible_labels(labels, &self.inner.ownership_labels(), None)
                    {
                        return Err(resource_conflict("container", &self.inner.name, reason));
                    }
                } else {
                    return Err(resource_conflict(
                        "container",
                        &self.inner.name,
                        "container labels are missing",
                    ));
                }
                return Err(resource_conflict(
                    "volume",
                    &self.inner.state_volume_name(),
                    "managed container exists but its state volume is missing",
                ));
            }
            (None, None) => {
                update_cleanup_state(&cleanup_state, |state| {
                    state.enabled = true;
                    state.volume_created = true;
                });
                let volume = self.inner.create_volume().await?;
                let resource_id = compatible_volume(&self.inner, &volume)?;
                if resource_id != self.inner.resource_id {
                    return Err(resource_conflict(
                        "volume",
                        &self.inner.state_volume_name(),
                        "volume was created concurrently with a different resource identity",
                    ));
                }
                update_cleanup_state(&cleanup_state, |state| state.container_created = true);
                self.inner.create(&host_config).await?;
            }
        }

        debug!("starting container {}", &self.inner.name);

        let start_result: Result<(), GrpcError> = async {
            if !matches!(
                self.inner.state().await?,
                Some(ContainerStateStatusEnum::RUNNING)
                    | Some(ContainerStateStatusEnum::RESTARTING)
            ) {
                self.inner.start().await?;
            }
            self.inner.wait().await
        }
        .await;
        if let Err(error) = start_result {
            if let Err(teardown_error) = tear_down_guard.tear_down().await {
                warn!("failed to tear down BuildKit container after bootstrap failure: {teardown_error}");
            }
            return Err(error);
        }

        tear_down_guard.disarm();
        Ok(self.inner)
    }

    /// The network mode to apply to the `Buildkit` docker container.
    pub fn network(&mut self, net: &str) -> &mut DockerContainerBuilder {
        if net == "host" {
            self.inner
                .args
                .push(String::from("--allow-insecure-entitlement=network.host"));
        }

        self.inner.net_mode = Some(net.to_string());
        self
    }

    /// The image to use when spinning up a `Buildkit` container. The default is [`DEFAULT_IMAGE`]
    pub fn image(&mut self, image: &str) -> &mut DockerContainerBuilder {
        self.inner.image = Some(String::from(image));
        self
    }

    /// The cgroup to attach to - by default all `Buildkit` containers are placed under the same
    /// cgroup so that limits are applied across the whole host
    pub fn cgroup_parent(&mut self, cgroup_parent: &str) -> &mut DockerContainerBuilder {
        self.inner.cgroup_parent = Some(String::from(cgroup_parent));
        self
    }

    /// Set an env variable for the `Buildkit` container.
    pub fn env(&mut self, env: &str) -> &mut DockerContainerBuilder {
        self.inner.env.push(String::from(env));
        self
    }

    /// Set a additional run command arguments to the `Buildkit` docker execution.
    pub fn arg(&mut self, arg: &str) -> &mut DockerContainerBuilder {
        self.inner.args.push(String::from(arg));
        self
    }

    /// Set the stable name used for the BuildKit container and state volume.
    ///
    /// The name is validated when [`Self::bootstrap`] is called. If this method
    /// is not used, a unique name is generated automatically.
    pub fn name(&mut self, name: &str) -> &mut DockerContainerBuilder {
        self.inner.name = String::from(name);
        self
    }

    /// Set the lifecycle policy for the BuildKit container.
    ///
    /// The default policy is [`DockerContainerLifecycle::Persistent`].
    pub fn lifecycle(
        &mut self,
        lifecycle: DockerContainerLifecycle,
    ) -> &mut DockerContainerBuilder {
        self.inner.lifecycle = lifecycle;
        self
    }
}

/// DockerContainer plumbing to communicate with `Buildkit` using an execution pipe.
/// Underneath, the `buildkit` CLI will open a stdin/stdout pipe, which we can hook into to call
/// further GRPC methods.
///
/// Construct a `DockerContainer` using a [`DockerContainerBuilder`]. A persistent driver can be
/// borrowed for multiple solves; each solve opens a fresh Docker exec transport and BuildKit
/// session while retaining the daemon and state volume.
///
///
#[derive(Debug)]
pub struct DockerContainer {
    name: String,
    docker: Docker,
    net_mode: Option<String>,
    image: Option<String>,
    cgroup_parent: Option<String>,
    env: Vec<String>,
    args: Vec<String>,
    lifecycle: DockerContainerLifecycle,
    resource_id: String,
}

impl super::Driver for DockerContainer {
    async fn grpc_handle(
        &self,
        session_id: &str,
        services: Vec<GrpcServer>,
    ) -> Result<ControlClient<InterceptedService<Channel, DriverInterceptor>>, GrpcError> {
        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(DockerContainerConnector {
                client: Docker::clone(&self.docker),
                name: String::clone(&self.name),
            })
            .await?;

        let channel = BuildkitChannel::new(channel);

        channel.grpc_handle(session_id, services).await
    }

    fn get_tear_down_handler(&self) -> Box<dyn super::DriverTearDownHandler> {
        match self.lifecycle {
            DockerContainerLifecycle::Persistent => Box::new(NoopTearDownHandler {}),
            DockerContainerLifecycle::StopAfterSolve => Box::new(DockerContainerTearDownHandler {
                name: String::from(&self.name),
                volume_name: self.state_volume_name(),
                resource_id: String::clone(&self.resource_id),
                expected_labels: self.ownership_labels(),
                docker: Docker::clone(&self.docker),
                operation: DockerContainerTearDownOperation::Stop,
            }),
            DockerContainerLifecycle::RemoveAfterSolve { keep_state } => {
                Box::new(DockerContainerTearDownHandler {
                    name: String::from(&self.name),
                    volume_name: self.state_volume_name(),
                    resource_id: String::clone(&self.resource_id),
                    expected_labels: self.ownership_labels(),
                    docker: Docker::clone(&self.docker),
                    operation: DockerContainerTearDownOperation::Remove { keep_state },
                })
            }
        }
    }
}

impl DockerContainer {
    /// Identifies the docker container name that runs `Buildkit`. This should be unique if you
    /// intend to run multiple instances building in parallel on the same host.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Stop the managed BuildKit container without removing its resources.
    ///
    /// The operation is idempotent when the container is already stopped or
    /// absent. A present container must still carry this driver's ownership
    /// labels and resource identity.
    pub async fn stop(&self) -> Result<(), GrpcError> {
        let expected = self.ownership_labels();
        let container =
            inspect_owned_container(&self.docker, &self.name, &expected, &self.resource_id).await?;

        let Some(container) = container else {
            return Ok(());
        };

        match stop_owned_container(&self.docker, &self.name, container).await? {
            StopResult::Stopped => Ok(()),
            StopResult::TimedOut => Err(operation_timeout("stop", &self.name)),
        }
    }

    /// Remove the managed BuildKit container and optionally its state volume.
    ///
    /// Removal is idempotent when the requested resources are already absent.
    /// Ownership is validated before any resource is stopped or removed. The
    /// container is stopped gracefully when necessary; a forced container
    /// removal is used only when that bounded stop operation times out.
    pub async fn remove(&self, options: DockerContainerRemoveOptions) -> Result<(), GrpcError> {
        remove_owned_resources(
            &self.docker,
            &self.name,
            &self.state_volume_name(),
            &self.ownership_labels(),
            &self.resource_id,
            options.keep_state,
        )
        .await
    }

    fn state_volume_name(&self) -> String {
        format!("{}_state", &self.name)
    }

    fn ownership_labels(&self) -> HashMap<String, String> {
        HashMap::from([
            (String::from(LABEL_MANAGED), String::from("true")),
            (String::from(LABEL_DRIVER), String::from("docker-container")),
            (
                String::from(LABEL_LIFECYCLE_SCHEMA),
                String::from(LIFECYCLE_SCHEMA_VERSION),
            ),
            (String::from(LABEL_BUILDER_NAME), self.name.clone()),
            (String::from(LABEL_STATE_VOLUME), self.state_volume_name()),
            (String::from(LABEL_RESOURCE_ID), self.resource_id.clone()),
        ])
    }

    fn restart_policy(&self) -> Option<RestartPolicy> {
        match self.lifecycle {
            DockerContainerLifecycle::Persistent => Some(RestartPolicy {
                name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
                ..Default::default()
            }),
            DockerContainerLifecycle::StopAfterSolve
            | DockerContainerLifecycle::RemoveAfterSolve { .. } => None,
        }
    }

    #[cfg(test)]
    fn desired_host_config_for_test(&self) -> HostConfig {
        HostConfig {
            privileged: Some(true),
            mounts: Some(vec![Mount {
                typ: Some(MountType::VOLUME),
                source: Some(self.state_volume_name()),
                target: Some(String::from(DEFAULT_STATE_DIR)),
                ..Default::default()
            }]),
            init: Some(true),
            network_mode: self.net_mode.clone(),
            restart_policy: self.restart_policy(),
            ..Default::default()
        }
    }

    async fn desired_host_config(&self) -> Result<HostConfig, GrpcError> {
        let info = self.docker.info().await?;
        let cgroup_parent = match &info.cgroup_driver {
            Some(SystemInfoCgroupDriverEnum::CGROUPFS) => Some(
                self.cgroup_parent
                    .clone()
                    .unwrap_or_else(|| String::from("/docker/buildx")),
            ),
            _ => None,
        };

        let userns_mode = info.security_options.as_ref().and_then(|security_options| {
            security_options
                .iter()
                .any(|option| option == "userns")
                .then(|| String::from("host"))
        });

        Ok(HostConfig {
            privileged: Some(true),
            mounts: Some(vec![Mount {
                typ: Some(MountType::VOLUME),
                source: Some(self.state_volume_name()),
                target: Some(String::from(DEFAULT_STATE_DIR)),
                ..Default::default()
            }]),
            init: Some(true),
            network_mode: self.net_mode.clone(),
            cgroup_parent,
            userns_mode,
            restart_policy: self.restart_policy(),
            ..Default::default()
        })
    }

    async fn create_volume(&self) -> Result<Volume, GrpcError> {
        let volume = self
            .docker
            .create_volume(VolumeCreateRequest {
                name: Some(self.state_volume_name()),
                labels: Some(self.ownership_labels()),
                ..Default::default()
            })
            .await?;
        Ok(volume)
    }

    async fn create(&self, host_config: &HostConfig) -> Result<(), GrpcError> {
        let image_name = if let Some(image) = &self.image {
            image
        } else {
            DEFAULT_IMAGE
        };

        debug!("pulling image {}", &image_name);

        // TODO: registry auth

        let create_image_options =
            bollard_stubs::query_parameters::CreateImageOptionsBuilder::default()
                .from_image(image_name)
                .build();

        self.docker
            .create_image(Some(create_image_options), None, None)
            .try_collect::<Vec<_>>()
            .await?;

        debug!("creating container {}", &self.name);

        let container_options =
            bollard_stubs::query_parameters::CreateContainerOptionsBuilder::default()
                .name(&self.name)
                .build();

        let container_config = ContainerCreateBody {
            image: Some(String::from(image_name)),
            env: Some(Vec::clone(&self.env)),
            host_config: Some(host_config.clone()),
            cmd: Some(Vec::clone(&self.args)),
            labels: Some(self.ownership_labels()),
            ..Default::default()
        };

        self.docker
            .create_container(Some(container_options), container_config)
            .await?;

        Ok(())
    }

    async fn start(&self) -> Result<(), GrpcError> {
        self.docker
            .start_container(
                &self.name,
                None::<crate::query_parameters::StartContainerOptions>,
            )
            .await?;

        Ok(())
    }

    async fn state(&self) -> Result<Option<ContainerStateStatusEnum>, GrpcError> {
        Ok(inspect_container(&self.docker, &self.name)
            .await?
            .and_then(|container| container.state.and_then(|state| state.status)))
    }

    async fn wait(&self) -> Result<(), GrpcError> {
        let mut attempts = 1;
        let mut stdout = BytesMut::new();
        loop {
            let exec = self
                .docker
                .create_exec(
                    &self.name,
                    CreateExecOptions {
                        attach_stdout: Some(true),
                        attach_stderr: Some(true),
                        cmd: Some(vec!["buildctl", "debug", "workers"]),
                        ..Default::default()
                    },
                )
                .await?
                .id;

            if let StartExecResults::Attached {
                mut output,
                input: _,
            } = self.docker.start_exec(&exec, None).await?
            {
                while let Some(Ok(output)) = output.next().await {
                    stdout.extend_from_slice(output.into_bytes().as_ref());
                }
            };

            let inspect: ExecInspectResponse = self.docker.inspect_exec(&exec).await?;

            match inspect {
                ExecInspectResponse {
                    exit_code: Some(0), ..
                } => return Ok(()),
                ExecInspectResponse {
                    exit_code: Some(status_code),
                    ..
                } if attempts > 15 => {
                    info!("{}", std::str::from_utf8(stdout.as_ref())?);
                    return Err(crate::errors::Error::DockerContainerWaitError {
                        error: String::from(std::str::from_utf8(stdout.as_ref())?),
                        code: status_code,
                    }
                    .into());
                }
                _ => {
                    tokio::time::sleep(Duration::from_millis(attempts * 120)).await;
                    attempts += 1;
                }
            }
        }
    }
}

async fn inspect_container(
    docker: &Docker,
    name: &str,
) -> Result<Option<ContainerInspectResponse>, GrpcError> {
    match docker.inspect_container(name, None).await {
        Ok(container) => Ok(Some(container)),
        Err(crate::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn inspect_volume(docker: &Docker, name: &str) -> Result<Option<Volume>, GrpcError> {
    match docker.inspect_volume(name).await {
        Ok(volume) => Ok(Some(volume)),
        Err(crate::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn inspect_owned_container(
    docker: &Docker,
    name: &str,
    expected_labels: &HashMap<String, String>,
    resource_id: &str,
) -> Result<Option<ContainerInspectResponse>, GrpcError> {
    let Some(container) = inspect_container(docker, name).await? else {
        return Ok(None);
    };

    validate_owned_container(&container, name, expected_labels, resource_id)?;

    Ok(Some(container))
}

fn validate_owned_container(
    container: &ContainerInspectResponse,
    name: &str,
    expected_labels: &HashMap<String, String>,
    resource_id: &str,
) -> Result<(), GrpcError> {
    let labels = container
        .config
        .as_ref()
        .and_then(|config| config.labels.as_ref())
        .ok_or_else(|| resource_conflict("container", name, "container labels are missing"))?;
    if let Err(reason) = compatible_labels(labels, expected_labels, Some(resource_id)) {
        return Err(resource_conflict("container", name, reason));
    }

    Ok(())
}

fn validate_owned_volume(
    volume: &Volume,
    name: &str,
    expected_labels: &HashMap<String, String>,
    resource_id: &str,
) -> Result<(), GrpcError> {
    if let Err(reason) = compatible_labels(&volume.labels, expected_labels, Some(resource_id)) {
        return Err(resource_conflict("volume", name, reason));
    }
    if volume.driver != "local" {
        return Err(resource_conflict(
            "volume",
            name,
            format!("volume driver `{}` is not `local`", volume.driver),
        ));
    }

    Ok(())
}

enum StopResult {
    Stopped,
    TimedOut,
}

async fn stop_owned_container(
    docker: &Docker,
    name: &str,
    container: ContainerInspectResponse,
) -> Result<StopResult, GrpcError> {
    if matches!(
        container.state.as_ref().and_then(|state| state.status),
        Some(
            ContainerStateStatusEnum::CREATED
                | ContainerStateStatusEnum::EXITED
                | ContainerStateStatusEnum::DEAD
                | ContainerStateStatusEnum::REMOVING
        )
    ) {
        return Ok(StopResult::Stopped);
    }

    let container_id = container.id.as_deref().unwrap_or(name);
    let stop = docker.stop_container(
        container_id,
        Some(
            bollard_stubs::query_parameters::StopContainerOptionsBuilder::default()
                .t(DOCKER_STOP_GRACE_PERIOD as i32)
                .build(),
        ),
    );

    match tokio::time::timeout(DOCKER_STOP_TIMEOUT, stop).await {
        Ok(Ok(())) => Ok(StopResult::Stopped),
        Ok(Err(error)) if is_not_found_docker(&error) => Ok(StopResult::Stopped),
        Ok(Err(error)) => Err(error.into()),
        Err(_) => Ok(StopResult::TimedOut),
    }
}

async fn remove_owned_resources(
    docker: &Docker,
    name: &str,
    volume_name: &str,
    expected_labels: &HashMap<String, String>,
    resource_id: &str,
    keep_state: bool,
) -> Result<(), GrpcError> {
    let container = inspect_owned_container(docker, name, expected_labels, resource_id).await?;
    let volume = if keep_state {
        None
    } else {
        let volume = inspect_volume(docker, volume_name).await?;
        if let Some(volume) = &volume {
            validate_owned_volume(volume, volume_name, expected_labels, resource_id)?;
        }
        volume
    };

    if let Some(container) = container {
        let container_id = container.id.clone().unwrap_or_else(|| name.to_string());
        let force = matches!(
            stop_owned_container(docker, name, container).await?,
            StopResult::TimedOut
        );
        let remove = docker.remove_container(
            &container_id,
            Some(
                bollard_stubs::query_parameters::RemoveContainerOptionsBuilder::default()
                    .force(force)
                    .build(),
            ),
        );
        match remove.await {
            Ok(()) => {}
            Err(error) if is_not_found_docker(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }

    if volume.is_some() {
        match docker
            .remove_volume(
                volume_name,
                Some(
                    bollard_stubs::query_parameters::RemoveVolumeOptionsBuilder::default().build(),
                ),
            )
            .await
        {
            Ok(()) => {}
            Err(error) if is_not_found_docker(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }

    Ok(())
}

fn operation_timeout(operation: &str, name: &str) -> GrpcError {
    GrpcError::DockerContainerOperationTimeout {
        operation: operation.to_string(),
        name: name.to_string(),
    }
}

fn is_not_found_docker(error: &crate::errors::Error) -> bool {
    matches!(
        error,
        crate::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
        }
    )
}

fn resource_conflict(resource: &str, name: &str, reason: impl Into<String>) -> GrpcError {
    GrpcError::DockerContainerResourceConflict {
        resource: resource.to_string(),
        name: name.to_string(),
        reason: reason.into(),
    }
}

fn ownership_value<'a>(labels: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    labels.get(key).map(String::as_str)
}

fn compatible_labels(
    labels: &HashMap<String, String>,
    expected: &HashMap<String, String>,
    resource_id: Option<&str>,
) -> Result<(), String> {
    for key in [
        LABEL_MANAGED,
        LABEL_DRIVER,
        LABEL_LIFECYCLE_SCHEMA,
        LABEL_BUILDER_NAME,
        LABEL_STATE_VOLUME,
    ] {
        if ownership_value(labels, key) != expected.get(key).map(String::as_str) {
            return Err(format!("ownership label `{key}` does not match"));
        }
    }

    let actual_resource_id = ownership_value(labels, LABEL_RESOURCE_ID)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("resource ID label is missing"))?;
    if let Some(resource_id) = resource_id {
        if actual_resource_id != resource_id {
            return Err(String::from("resource ID label does not match"));
        }
    }

    Ok(())
}

fn compatible_volume(container: &DockerContainer, volume: &Volume) -> Result<String, GrpcError> {
    let labels = &volume.labels;
    if let Err(reason) = compatible_labels(labels, &container.ownership_labels(), None) {
        return Err(resource_conflict("volume", &volume.name, reason));
    }
    if volume.driver != "local" {
        return Err(resource_conflict(
            "volume",
            &volume.name,
            format!("volume driver `{}` is not `local`", volume.driver),
        ));
    }

    Ok(labels
        .get(LABEL_RESOURCE_ID)
        .expect("compatible_labels validated the resource ID")
        .clone())
}

fn compatible_container(
    container: &DockerContainer,
    inspect: &ContainerInspectResponse,
    host_config: &HostConfig,
    resource_id: &str,
) -> Result<(), GrpcError> {
    let config = inspect.config.as_ref().ok_or_else(|| {
        resource_conflict("container", &container.name, "container config is missing")
    })?;
    if let Err(reason) = compatible_labels(
        config.labels.as_ref().ok_or_else(|| {
            resource_conflict("container", &container.name, "container labels are missing")
        })?,
        &container.ownership_labels(),
        Some(resource_id),
    ) {
        return Err(resource_conflict("container", &container.name, reason));
    }

    let expected_image = container.image.as_deref().unwrap_or(DEFAULT_IMAGE);
    if config.image.as_deref() != Some(expected_image) {
        return Err(resource_conflict(
            "container",
            &container.name,
            "image reference does not match",
        ));
    }

    if config.cmd.as_deref().unwrap_or_default() != container.args.as_slice() {
        return Err(resource_conflict(
            "container",
            &container.name,
            "BuildKit daemon arguments do not match",
        ));
    }

    let existing_env = config.env.as_deref().unwrap_or_default();
    for requested in &container.env {
        if let Some((key, _)) = requested.split_once('=') {
            if !existing_env.iter().any(|entry| entry == requested) {
                return Err(resource_conflict(
                    "container",
                    &container.name,
                    format!("environment override `{key}` does not match"),
                ));
            }
        } else if existing_env.iter().any(|entry| {
            entry
                .split_once('=')
                .is_some_and(|(key, _)| key == requested)
        }) {
            return Err(resource_conflict(
                "container",
                &container.name,
                format!("environment variable `{requested}` was expected to be removed"),
            ));
        }
    }

    let existing_host_config = inspect.host_config.as_ref().ok_or_else(|| {
        resource_conflict(
            "container",
            &container.name,
            "host configuration is missing",
        )
    })?;
    if normalized_host_config_value(existing_host_config.network_mode.as_ref())
        != normalized_host_config_value(host_config.network_mode.as_ref())
        || normalized_host_config_value(existing_host_config.cgroup_parent.as_ref())
            != normalized_host_config_value(host_config.cgroup_parent.as_ref())
        || normalized_host_config_value(existing_host_config.userns_mode.as_ref())
            != normalized_host_config_value(host_config.userns_mode.as_ref())
    {
        return Err(resource_conflict(
            "container",
            &container.name,
            "host namespace configuration does not match",
        ));
    }
    if existing_host_config.privileged != Some(true) || existing_host_config.init != Some(true) {
        return Err(resource_conflict(
            "container",
            &container.name,
            "container must be privileged and use init",
        ));
    }

    let expected_restart = host_config
        .restart_policy
        .as_ref()
        .and_then(|policy| policy.name);
    let actual_restart = existing_host_config
        .restart_policy
        .as_ref()
        .and_then(|policy| policy.name)
        .filter(|policy| {
            !matches!(
                policy,
                RestartPolicyNameEnum::EMPTY | RestartPolicyNameEnum::NO
            )
        });
    if actual_restart != expected_restart {
        return Err(resource_conflict(
            "container",
            &container.name,
            "restart policy does not match",
        ));
    }

    let matching_mounts = inspect
        .mounts
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|mount| {
            mount.typ.as_deref() == Some("volume")
                && mount.name.as_deref() == Some(container.state_volume_name().as_str())
                && mount.destination.as_deref() == Some(DEFAULT_STATE_DIR)
                && mount.rw != Some(false)
        })
        .count();
    if matching_mounts != 1 {
        return Err(resource_conflict(
            "container",
            &container.name,
            "state volume mount does not match",
        ));
    }

    Ok(())
}

fn normalized_host_config_value(value: Option<&String>) -> Option<&str> {
    value.map(String::as_str).filter(|value| !value.is_empty())
}

fn update_cleanup_state(
    state: &Arc<Mutex<BootstrapCleanupState>>,
    update: impl FnOnce(&mut BootstrapCleanupState),
) {
    if let Ok(mut state) = state.lock() {
        update(&mut state);
    }
}

#[derive(Debug)]
struct BootstrapCleanupState {
    enabled: bool,
    container_created: bool,
    volume_created: bool,
    name: String,
    volume_name: String,
    resource_id: String,
}

struct BootstrapTearDownHandler {
    docker: Docker,
    state: Arc<Mutex<BootstrapCleanupState>>,
}

impl super::DriverTearDownHandler for BootstrapTearDownHandler {
    fn tear_down(&self) -> Pin<Box<dyn Future<Output = Result<(), GrpcError>> + Send + 'static>> {
        let docker = Docker::clone(&self.docker);
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            let (enabled, container_created, volume_created, name, volume_name, resource_id) = {
                let state = state
                    .lock()
                    .map_err(|_| tonic::Status::internal("bootstrap cleanup state was poisoned"))?;
                (
                    state.enabled,
                    state.container_created,
                    state.volume_created,
                    state.name.clone(),
                    state.volume_name.clone(),
                    state.resource_id.clone(),
                )
            };
            if !enabled {
                return Ok(());
            }

            if container_created {
                if let Some(container) = inspect_container(&docker, &name).await? {
                    let owned = container
                        .config
                        .as_ref()
                        .and_then(|config| config.labels.as_ref())
                        .and_then(|labels| labels.get(LABEL_RESOURCE_ID))
                        == Some(&resource_id);
                    if owned {
                        docker
                            .remove_container(
                                &name,
                                Some(
                                    bollard_stubs::query_parameters::RemoveContainerOptionsBuilder::default()
                                        .force(true)
                                        .build(),
                                ),
                            )
                            .await?;
                    }
                }
            }

            if volume_created {
                if let Some(volume) = inspect_volume(&docker, &volume_name).await? {
                    let owned = volume.labels.get(LABEL_RESOURCE_ID) == Some(&resource_id);
                    if owned {
                        docker
                            .remove_volume(
                                &volume_name,
                                Some(
                                    bollard_stubs::query_parameters::RemoveVolumeOptionsBuilder::default()
                                        .force(true)
                                        .build(),
                                ),
                            )
                            .await?;
                    }
                }
            }

            Ok(())
        })
    }
}

fn validate_container_name(name: &str) -> Result<(), GrpcError> {
    if name.chars().count() < 2 {
        return Err(tonic::Status::invalid_argument(format!(
            "invalid Docker-container builder name `{name}`: expected [a-zA-Z0-9][a-zA-Z0-9_.-]+"
        ))
        .into());
    }

    let mut characters = name.chars();
    let first = characters.next().expect("validated container name length");

    if !first.is_ascii_alphanumeric()
        || !characters
            .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character))
    {
        return Err(tonic::Status::invalid_argument(format!(
            "invalid Docker-container builder name `{name}`: expected [a-zA-Z0-9][a-zA-Z0-9_.-]+"
        ))
        .into());
    }

    Ok(())
}

struct DockerContainerTearDownHandler {
    name: String,
    volume_name: String,
    resource_id: String,
    expected_labels: HashMap<String, String>,
    docker: Docker,
    operation: DockerContainerTearDownOperation,
}

#[derive(Debug, Clone, Copy)]
enum DockerContainerTearDownOperation {
    Stop,
    Remove { keep_state: bool },
}

impl super::DriverTearDownHandler for DockerContainerTearDownHandler {
    fn tear_down(&self) -> Pin<Box<dyn Future<Output = Result<(), GrpcError>> + Send + 'static>> {
        let docker = Docker::clone(&self.docker);
        let name = String::clone(&self.name);
        let volume_name = String::clone(&self.volume_name);
        let resource_id = String::clone(&self.resource_id);
        let expected_labels = self.expected_labels.clone();
        let operation = self.operation;
        Box::pin(async move {
            match operation {
                DockerContainerTearDownOperation::Stop => {
                    let Some(container) =
                        inspect_owned_container(&docker, &name, &expected_labels, &resource_id)
                            .await?
                    else {
                        return Ok(());
                    };
                    if matches!(
                        stop_owned_container(&docker, &name, container).await?,
                        StopResult::TimedOut
                    ) {
                        return Err(operation_timeout("stop", &name));
                    }
                    Ok(())
                }
                DockerContainerTearDownOperation::Remove { keep_state } => {
                    remove_owned_resources(
                        &docker,
                        &name,
                        &volume_name,
                        &expected_labels,
                        &resource_id,
                        keep_state,
                    )
                    .await
                }
            }
        })
    }
}

struct NoopTearDownHandler {}

impl super::DriverTearDownHandler for NoopTearDownHandler {
    fn tear_down(
        &self,
    ) -> Pin<Box<dyn futures_core::Future<Output = Result<(), GrpcError>> + Send + 'static>> {
        Box::pin(futures_util::future::ok(()))
    }
}

impl super::Export for DockerContainer {
    async fn export(
        &self,
        exporter_request: ImageExporterEnum,
        frontend_opts: ImageBuildFrontendOptions,
        load_input: ImageBuildLoadInput,
        credentials: Option<HashMap<&str, DockerCredentials>>,
        build_ref: Option<BuildRef>,
    ) -> Result<(), GrpcError> {
        let (exporter, exporter_attrs, path) = match exporter_request {
            ImageExporterEnum::OCI(request) => ("oci", request.output.into_map(), request.path),
            ImageExporterEnum::Docker(request) => {
                ("docker", request.output.into_map(), request.path)
            }
        };
        super::solve(
            self,
            exporter,
            exporter_attrs,
            Some(path),
            frontend_opts,
            load_input,
            credentials,
            build_ref,
        )
        .await
    }
}

impl super::Image for DockerContainer {
    async fn registry(
        &self,
        output: ImageRegistryOutput,
        frontend_opts: ImageBuildFrontendOptions,
        load_input: ImageBuildLoadInput,
        credentials: Option<HashMap<&str, DockerCredentials>>,
        build_ref: Option<BuildRef>,
    ) -> Result<(), GrpcError> {
        let exporter = "image";
        let exporter_attrs = output.into_map();
        super::solve(
            self,
            exporter,
            exporter_attrs,
            None,
            frontend_opts,
            load_input,
            credentials,
            build_ref,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard_stubs::models::{ContainerConfig, ContainerState, MountPoint};

    fn builder() -> DockerContainerBuilder {
        let docker = Docker::connect_with_http_defaults().unwrap();
        DockerContainerBuilder::new(&docker)
    }

    #[test]
    fn persistent_is_the_default_lifecycle() {
        let builder = builder();

        assert_eq!(
            builder.inner.lifecycle,
            DockerContainerLifecycle::Persistent
        );
    }

    #[test]
    fn lifecycle_policy_can_be_selected() {
        let mut builder = builder();

        builder.lifecycle(DockerContainerLifecycle::RemoveAfterSolve { keep_state: true });

        assert_eq!(
            builder.inner.lifecycle,
            DockerContainerLifecycle::RemoveAfterSolve { keep_state: true }
        );
    }

    #[test]
    fn remove_options_delete_state_by_default() {
        assert!(!DockerContainerRemoveOptions::new().keep_state);
        assert!(
            DockerContainerRemoveOptions::new()
                .keep_state(true)
                .keep_state
        );
    }

    #[test]
    fn name_setter_updates_container_identity() {
        let mut builder = builder();

        builder.name("project-builder");

        assert_eq!(builder.inner.name, "project-builder");
        assert_eq!(builder.inner.state_volume_name(), "project-builder_state");
    }

    #[test]
    fn valid_container_names_are_accepted() {
        for name in ["ab", "project-builder", "A_b.c-1"] {
            assert!(validate_container_name(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn invalid_container_names_are_rejected() {
        for name in [
            "",
            "a",
            "-builder",
            ".builder",
            "builder/name",
            "builder name",
            "büilder",
        ] {
            assert!(validate_container_name(name).is_err(), "{name}");
        }
    }

    #[test]
    fn ownership_labels_identify_the_builder_and_state_volume() {
        let mut builder = builder();
        builder.name("project-builder");
        let labels = builder.inner.ownership_labels();

        assert_eq!(labels.get(LABEL_MANAGED), Some(&String::from("true")));
        assert_eq!(
            labels.get(LABEL_DRIVER),
            Some(&String::from("docker-container"))
        );
        assert_eq!(
            labels.get(LABEL_LIFECYCLE_SCHEMA),
            Some(&String::from(LIFECYCLE_SCHEMA_VERSION))
        );
        assert_eq!(
            labels.get(LABEL_BUILDER_NAME),
            Some(&String::from("project-builder"))
        );
        assert_eq!(
            labels.get(LABEL_STATE_VOLUME),
            Some(&String::from("project-builder_state"))
        );
        assert_eq!(
            labels.get(LABEL_RESOURCE_ID),
            Some(&builder.inner.resource_id)
        );
    }

    #[test]
    fn compatible_labels_require_matching_resource_identity() {
        let builder = builder();
        let labels = builder.inner.ownership_labels();

        assert!(compatible_labels(&labels, &labels, Some(&builder.inner.resource_id)).is_ok());
        assert!(compatible_labels(&labels, &labels, Some("different-resource")).is_err());
    }

    #[test]
    fn compatible_volume_requires_bollard_ownership() {
        let mut builder = builder();
        builder.name("project-builder");
        let volume = Volume {
            name: builder.inner.state_volume_name(),
            driver: String::from("local"),
            labels: builder.inner.ownership_labels(),
            ..Default::default()
        };

        assert_eq!(
            compatible_volume(&builder.inner, &volume).unwrap(),
            builder.inner.resource_id
        );
    }

    #[test]
    fn explicit_management_requires_matching_volume_identity() {
        let mut builder = builder();
        builder.name("project-builder");
        let mut labels = builder.inner.ownership_labels();
        labels.insert(
            String::from(LABEL_RESOURCE_ID),
            String::from("different-resource"),
        );
        let volume = Volume {
            name: builder.inner.state_volume_name(),
            driver: String::from("local"),
            labels,
            ..Default::default()
        };

        assert!(validate_owned_volume(
            &volume,
            &volume.name,
            &builder.inner.ownership_labels(),
            &builder.inner.resource_id,
        )
        .is_err());
    }

    #[test]
    fn explicit_management_requires_matching_container_identity() {
        let mut builder = builder();
        builder.name("project-builder");
        let mut labels = builder.inner.ownership_labels();
        labels.insert(
            String::from(LABEL_RESOURCE_ID),
            String::from("different-resource"),
        );
        let container = ContainerInspectResponse {
            config: Some(ContainerConfig {
                labels: Some(labels),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!(validate_owned_container(
            &container,
            &builder.inner.name,
            &builder.inner.ownership_labels(),
            &builder.inner.resource_id,
        )
        .is_err());
    }

    #[test]
    fn compatible_container_requires_the_expected_state_mount() {
        let mut builder = builder();
        builder.name("project-builder");
        builder.network("host");

        let host_config = builder.inner.desired_host_config_for_test();
        let mut existing_host_config = host_config.clone();
        existing_host_config.cgroup_parent = Some(String::new());
        existing_host_config.userns_mode = Some(String::new());
        let inspect = ContainerInspectResponse {
            state: Some(ContainerState {
                status: Some(ContainerStateStatusEnum::RUNNING),
                ..Default::default()
            }),
            host_config: Some(existing_host_config),
            config: Some(ContainerConfig {
                image: Some(String::from(DEFAULT_IMAGE)),
                cmd: Some(Vec::clone(&builder.inner.args)),
                env: Some(Vec::clone(&builder.inner.env)),
                labels: Some(builder.inner.ownership_labels()),
                ..Default::default()
            }),
            mounts: Some(vec![MountPoint {
                typ: Some(String::from("volume")),
                name: Some(builder.inner.state_volume_name()),
                destination: Some(String::from(DEFAULT_STATE_DIR)),
                rw: Some(true),
                ..Default::default()
            }]),
            ..Default::default()
        };

        assert!(compatible_container(
            &builder.inner,
            &inspect,
            &host_config,
            &builder.inner.resource_id
        )
        .is_ok());
    }

    #[test]
    fn incompatible_container_image_is_rejected() {
        let mut builder = builder();
        builder.name("project-builder");
        let host_config = builder.inner.desired_host_config_for_test();
        let inspect = ContainerInspectResponse {
            host_config: Some(host_config.clone()),
            config: Some(ContainerConfig {
                image: Some(String::from("moby/buildkit:other")),
                cmd: Some(Vec::clone(&builder.inner.args)),
                labels: Some(builder.inner.ownership_labels()),
                ..Default::default()
            }),
            mounts: Some(vec![MountPoint {
                typ: Some(String::from("volume")),
                name: Some(builder.inner.state_volume_name()),
                destination: Some(String::from(DEFAULT_STATE_DIR)),
                rw: Some(true),
                ..Default::default()
            }]),
            ..Default::default()
        };

        assert!(compatible_container(
            &builder.inner,
            &inspect,
            &host_config,
            &builder.inner.resource_id
        )
        .is_err());
    }

    #[test]
    fn persistent_builders_restart_unless_stopped() {
        let builder = builder();

        assert_eq!(
            builder
                .inner
                .restart_policy()
                .and_then(|policy| policy.name),
            Some(RestartPolicyNameEnum::UNLESS_STOPPED)
        );
    }

    #[test]
    fn ephemeral_builders_do_not_restart_automatically() {
        let mut builder = builder();

        builder.lifecycle(DockerContainerLifecycle::StopAfterSolve);
        assert!(builder.inner.restart_policy().is_none());

        builder.lifecycle(DockerContainerLifecycle::RemoveAfterSolve { keep_state: false });
        assert!(builder.inner.restart_policy().is_none());
    }
}
