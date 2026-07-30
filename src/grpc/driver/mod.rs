use std::time::Duration;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Instant,
};

use bollard_buildkit_proto::moby::{
    buildkit::{
        secrets::v1::secrets_server::SecretsServer,
        v1::{control_client::ControlClient, CacheOptions, Exporter, SolveRequest},
    },
    filesync::{
        packet::file_send_server::FileSendServer as FileSendPacketServer,
        packet::file_sync_server::FileSyncServer,
        v1::{auth_server::AuthServer, file_send_server::FileSendServer},
    },
    sshforward::v1::ssh_server::SshServer,
    upload::v1::upload_server::UploadServer,
};
use futures_util::TryFutureExt;
use log::{debug, warn};
// use tonic::service::Interceptor;
use tonic::{
    codegen::InterceptedService, metadata::MetadataValue, service::Interceptor, transport::Channel,
};

use crate::{
    auth::DockerCredentials,
    grpc::{
        build::{ImageBuildFrontendOptionsIngest, SecretSource},
        BuildRef, FileTransferLimits,
    },
};

use super::{
    build::{ImageBuildFrontendOptions, ImageBuildLoadInput},
    error::GrpcError,
    export::ImageExporterRequest,
    registry::ImageRegistryOutput,
    GrpcServer,
};

/// DEFAULT_MAX_SEND_MSG_SIZE defines the default maximum message size for
/// sending protobufs passed over the GRPC API.
/// See https://github.com/containerd/containerd/blob/997f813b5cfdd7e120ee60d93b83ac6babbcfb1a/defaults/defaults.go#L23-L25
/// Used by buildkit [here](https://github.com/moby/buildkit/blob/082e8d8cf3267ddd3a28de1e258eaec20ebe3bbe/cmd/buildkitd/main.go#L310)
pub(crate) const DEFAULT_MAX_SEND_MSG_SIZE: usize = 16 << 20;
/// DEFAULT_MAX_RECV_MSG_SIZE defines the default maximum message size for
/// receiving protobufs passed over the GRPC API.
/// See https://github.com/containerd/containerd/blob/997f813b5cfdd7e120ee60d93b83ac6babbcfb1a/defaults/defaults.go#L20-L22
/// Used by buildkit [here](https://github.com/moby/buildkit/blob/082e8d8cf3267ddd3a28de1e258eaec20ebe3bbe/cmd/buildkitd/main.go#L309)
pub(crate) const DEFAULT_MAX_RECV_MSG_SIZE: usize = 16 << 20;
const TEAR_DOWN_TIMEOUT: Duration = Duration::from_secs(30);

/// The Buildkit Daemon driver opens a GRPC connection by connecting to a Buildkit Daemon over a TCP connection.
pub mod buildkitd;
/// The Buildkit Channel driver opens a GRPC connection by using an existing [`tonic::transport::Channel`]
pub mod channel;
/// The Docker Container driver opens a GRPC connection by instantiating a Buildkit container over
/// the traditional docker socket, and communicating over a docker execution Stdin/Stdout pipe.
pub mod docker_container;
/// The Moby driver opens a bi-directional GRPC connection by upgrading HTTP `/session` and `/grpc`
/// endpoints over the traditional docker socket.
pub mod moby;

pub(crate) trait Driver {
    async fn grpc_handle(
        &self,
        session_id: &str,
        services: Vec<GrpcServer>,
    ) -> Result<ControlClient<InterceptedService<Channel, DriverInterceptor>>, GrpcError>;
    fn begin_solve(&self) -> Result<Box<dyn DriverTearDownHandler>, GrpcError>;
}

pub(crate) trait DriverTearDownHandler: Send + Sync {
    fn tear_down(
        &self,
    ) -> std::pin::Pin<Box<dyn futures_core::Future<Output = Result<(), GrpcError>> + Send + 'static>>;
}

struct TearDownGuard {
    handler: Arc<dyn DriverTearDownHandler>,
    runtime: tokio::runtime::Handle,
    task: Option<tokio::task::JoinHandle<Result<(), GrpcError>>>,
    armed: bool,
    started: bool,
}

impl TearDownGuard {
    fn new(handler: Box<dyn DriverTearDownHandler>) -> Self {
        Self {
            handler: Arc::from(handler),
            runtime: tokio::runtime::Handle::current(),
            task: None,
            armed: true,
            started: false,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    fn start(&mut self) {
        if self.started {
            return;
        }

        self.started = true;
        let handler = Arc::clone(&self.handler);
        self.task = Some(self.runtime.spawn(run_tear_down(handler)));
    }

    async fn tear_down(&mut self) -> Result<(), GrpcError> {
        self.start();
        self.task
            .take()
            .ok_or_else(|| GrpcError::TearDownTaskUnavailable)?
            .await
            .map_err(|error| tonic::Status::internal(format!("teardown task failed: {error}")))?
    }
}

async fn run_tear_down(handler: Arc<dyn DriverTearDownHandler>) -> Result<(), GrpcError> {
    let task = tokio::spawn(async move { handler.tear_down().await });
    match tokio::time::timeout(TEAR_DOWN_TIMEOUT, task).await {
        Ok(result) => result
            .map_err(|error| tonic::Status::internal(format!("teardown task failed: {error}")))?,
        Err(_) => Err(GrpcError::from(tonic::Status::deadline_exceeded(
            "driver teardown exceeded its timeout; cleanup continues in background",
        ))),
    }
}

impl Drop for TearDownGuard {
    fn drop(&mut self) {
        if !self.armed || self.started {
            // Dropping a JoinHandle detaches the task, allowing cleanup to finish after the
            // solve future is cancelled or unwinds due to a panic.
            return;
        }

        let handler = Arc::clone(&self.handler);
        self.runtime.spawn(async move {
            if let Err(error) = run_tear_down(handler).await {
                warn!("failed to tear down BuildKit driver after cancellation: {error}");
            }
        });
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DriverInterceptor {
    session_id: String,
    metadata_grpc_method: Vec<String>,
}

impl Interceptor for DriverInterceptor {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        let metadata = req.metadata_mut();

        metadata.insert(
            "x-docker-expose-session-uuid",
            self.session_id
                .parse()
                .map_err(|_| tonic::Status::invalid_argument("invalid 'session_id' argument"))?,
        );

        debug!("grpc-method: {:?}", self.metadata_grpc_method.join(","));
        for metadata_grpc_method_value in &self.metadata_grpc_method {
            let metadata_value = metadata_grpc_method_value
                .parse::<MetadataValue<tonic::metadata::Ascii>>()
                .map_err(|_| tonic::Status::invalid_argument("invalid grpc method name"))?;
            metadata.append("x-docker-expose-session-grpc-method", metadata_value);
        }

        Ok(req)
    }
}

/// Parameterises the [`docker_container::DockerContainer`] or [`moby::Moby`] driver with an exporter configuration. See
/// <https://docs.docker.com/build/exporters/oci-docker/>
#[derive(Debug, Clone)]
pub enum ImageExporterEnum {
    /// Export using the `oci` exporter.
    OCI(ImageExporterRequest),
    /// Export using the `docker` exporter.
    Docker(ImageExporterRequest),
}

/// Exporter selection for a direct LLB definition solve.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum DefinitionExporter {
    /// Export the solved filesystem into a local directory.
    Local(PathBuf),
}

/// Options for a direct LLB definition solve.
#[derive(Debug, Clone, Default)]
pub struct DefinitionSolveOptions {
    cache_to: Vec<bollard_buildkit_proto::moby::buildkit::v1::CacheOptionsEntry>,
    cache_from: Vec<bollard_buildkit_proto::moby::buildkit::v1::CacheOptionsEntry>,
    credentials: HashMap<String, DockerCredentials>,
    secrets: HashMap<String, SecretSource>,
    ssh: bool,
    timeout: Option<Duration>,
    file_transfer_limits: FileTransferLimits,
    local_mounts: HashMap<String, PathBuf>,
}

/// Builder for direct LLB definition solve options.
#[derive(Debug, Clone, Default)]
pub struct DefinitionSolveOptionsBuilder {
    options: DefinitionSolveOptions,
}

impl DefinitionSolveOptionsBuilder {
    /// Construct empty direct-definition solve options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a cache export configuration.
    pub fn cache_to(
        mut self,
        cache: &bollard_buildkit_proto::moby::buildkit::v1::CacheOptionsEntry,
    ) -> Self {
        self.options.cache_to.push(cache.clone());
        self
    }

    /// Add a cache import configuration.
    pub fn cache_from(
        mut self,
        cache: &bollard_buildkit_proto::moby::buildkit::v1::CacheOptionsEntry,
    ) -> Self {
        self.options.cache_from.push(cache.clone());
        self
    }

    /// Configure credentials for a registry host.
    pub fn credential(mut self, host: impl Into<String>, credentials: DockerCredentials) -> Self {
        self.options.credentials.insert(host.into(), credentials);
        self
    }

    /// Expose a secret source under the given BuildKit secret ID.
    pub fn secret(mut self, id: impl Into<String>, source: SecretSource) -> Self {
        self.options.secrets.insert(id.into(), source);
        self
    }

    /// Enable SSH agent forwarding for the solve.
    pub fn enable_ssh(mut self, enable: bool) -> Self {
        self.options.ssh = enable;
        self
    }

    /// Set one wall-clock deadline for setup and the active solve.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.options.timeout = Some(timeout);
        self
    }

    /// Set aggregate packet-export transfer limits.
    pub fn file_transfer_limits(mut self, limits: FileTransferLimits) -> Self {
        self.options.file_transfer_limits = limits;
        self
    }

    /// Expose a host directory under a BuildKit `local://` source name.
    pub fn local_mount(mut self, name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.options.local_mounts.insert(name.into(), path.into());
        self
    }

    /// Consume the builder and return immutable solve options.
    pub fn build(self) -> DefinitionSolveOptions {
        self.options
    }
}

/// A direct-definition solve request.
#[derive(Debug, Clone)]
pub struct DefinitionSolveRequest {
    /// The pre-built LLB definition to solve.
    pub definition: bollard_buildkit_proto::pb::Definition,
    /// Where to export the result.
    pub exporter: DefinitionExporter,
    options: DefinitionSolveOptions,
    build_ref: Option<BuildRef>,
}

impl DefinitionSolveRequest {
    /// Construct a request for a definition and exporter.
    pub fn new(
        definition: bollard_buildkit_proto::pb::Definition,
        exporter: DefinitionExporter,
    ) -> Self {
        Self {
            definition,
            exporter,
            options: DefinitionSolveOptions::default(),
            build_ref: None,
        }
    }

    /// Set immutable solve options.
    pub fn with_options(mut self, options: DefinitionSolveOptions) -> Self {
        self.options = options;
        self
    }

    /// Set the BuildKit build reference.
    pub fn with_build_ref(mut self, build_ref: BuildRef) -> Self {
        self.build_ref = Some(build_ref);
        self
    }
}

/// Trait for solving a pre-built LLB definition without a frontend.
pub trait SolveDefinition {
    /// Solve a direct LLB definition and export its result.
    async fn solve_definition(&self, request: DefinitionSolveRequest) -> Result<(), GrpcError>;
}

/// Trait enabling container exports.
pub trait Export {
    /// Export the container to a tar
    async fn export(
        &self,
        exporter_request: ImageExporterEnum,
        frontend_opts: ImageBuildFrontendOptions,
        load_input: ImageBuildLoadInput,
        credentials: Option<HashMap<&str, DockerCredentials>>,
        build_ref: Option<BuildRef>,
    ) -> Result<(), GrpcError>;
}

/// Trait enabling docker builds.
pub trait Build {
    /// Build a docker container without exporting
    async fn docker_build(
        &self,
        name: &str,
        frontend_opts: ImageBuildFrontendOptions,
        load_input: ImageBuildLoadInput,
        credentials: Option<HashMap<&str, DockerCredentials>>,
        build_ref: Option<BuildRef>,
    ) -> Result<(), GrpcError>;
}

/// Trait enabling registry facilities
pub trait Image {
    /// Push a container build to the registry
    async fn registry(
        &self,
        output: ImageRegistryOutput,
        frontend_opts: ImageBuildFrontendOptions,
        load_input: ImageBuildLoadInput,
        credentials: Option<HashMap<&str, DockerCredentials>>,
        build_ref: Option<BuildRef>,
    ) -> Result<(), GrpcError>;
}

#[allow(
    clippy::too_many_arguments,
    reason = "The nature of this function requires many parameters, maybe we can eventually create a Request structure?"
)]
pub(crate) async fn solve(
    driver: &impl Driver,
    exporter: &str,
    exporter_attrs: HashMap<String, String>,
    path: Option<PathBuf>,
    frontend_opts: ImageBuildFrontendOptions,
    load_input: ImageBuildLoadInput,
    credentials: Option<HashMap<&str, DockerCredentials>>,
    build_ref: Option<super::BuildRef>,
) -> Result<(), GrpcError> {
    let session_id = crate::grpc::new_id();

    let ImageBuildLoadInput::Upload(payload) = load_input;

    let mut upload_provider = super::UploadProvider::new();
    let context = upload_provider.add(payload.to_vec());

    let ImageBuildFrontendOptionsIngest {
        cache_to,
        cache_from,
        mut frontend_attrs,
        secret_sources,
        ssh,
    } = frontend_opts.consume();

    frontend_attrs.insert(String::from("context"), context);

    let mut auth_provider = super::AuthProvider::new();
    if let Some(creds) = credentials {
        for (host, docker_credentials) in creds {
            auth_provider.set_docker_credentials(host, docker_credentials);
        }
    }

    let secret_provider = super::SecretProvider::new(secret_sources);

    let auth = AuthServer::new(auth_provider);
    let upload = UploadServer::new(upload_provider)
        .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
        .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);

    let secret = SecretsServer::new(secret_provider)
        .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
        .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);

    let mut services: Vec<GrpcServer> = vec![
        GrpcServer::Auth(auth),
        GrpcServer::Upload(upload),
        GrpcServer::Secrets(secret),
    ];

    if ssh {
        let ssh_provider = super::SshProvider::new();
        let ssh = SshServer::new(ssh_provider)
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);

        services.push(GrpcServer::Ssh(ssh));
    }

    if let Some(path) = path {
        let filesend = FileSendServer::new(super::FileSendImpl::new(path.as_path()))
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);

        services.push(GrpcServer::FileSend(filesend));
    }

    let tear_down_handler = driver.begin_solve()?;

    let id = build_ref.unwrap_or_default();

    let solve_request = SolveRequest {
        r#ref: id.into(),
        cache: Some(CacheOptions {
            export_ref_deprecated: String::new(),
            import_refs_deprecated: Vec::new(),
            export_attrs_deprecated: HashMap::new(),
            exports: cache_to,
            imports: cache_from,
        }),
        definition: None,
        entitlements: vec![],
        exporter_deprecated: String::from(exporter),
        exporter_attrs_deprecated: exporter_attrs,
        frontend: String::from("dockerfile.v0"),
        frontend_attrs,
        frontend_inputs: HashMap::new(),
        session: session_id.clone(),
        exporters: vec![],
        internal: false,
        source_policy: None,
        enable_session_exporter: false,
        source_policy_session: String::new(),
    };

    execute_solve(
        driver,
        &session_id,
        solve_request,
        services,
        tear_down_handler,
        None,
    )
    .await
}

pub(crate) async fn solve_definition(
    driver: &impl Driver,
    request: DefinitionSolveRequest,
) -> Result<(), GrpcError> {
    let session_id = crate::grpc::new_id();
    let deadline = request
        .options
        .timeout
        .map(|timeout| Instant::now() + timeout);

    let DefinitionSolveRequest {
        definition,
        exporter,
        options,
        build_ref,
    } = request;

    let local_mounts = run_until(
        deadline,
        validate_local_mounts(options.local_mounts.clone()),
    )
    .await?;

    let mut auth_provider = super::AuthProvider::new();
    for (host, credentials) in options.credentials.clone() {
        auth_provider.set_docker_credentials(&host, credentials);
    }

    let auth = AuthServer::new(auth_provider);
    let secret = SecretsServer::new(super::SecretProvider::new(options.secrets.clone()))
        .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
        .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);
    let mut services = vec![GrpcServer::Auth(auth), GrpcServer::Secrets(secret)];

    if options.ssh {
        let ssh = SshServer::new(super::SshProvider::new())
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);
        services.push(GrpcServer::Ssh(ssh));
    }

    match &exporter {
        DefinitionExporter::Local(path) => {
            let filesend = FileSendPacketServer::new(super::FileSendPacketImpl::with_limits(
                path,
                options.file_transfer_limits,
            ))
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);
            services.push(GrpcServer::FileSendPacket(filesend));
        }
    }

    if !local_mounts.is_empty() {
        let filesync = FileSyncServer::new(super::filesync::FileSyncImpl::new(local_mounts))
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);
        services.push(GrpcServer::FileSync(filesync));
    }

    let tear_down_handler = driver.begin_solve()?;
    let solve_request =
        build_definition_solve_request(definition, &exporter, &options, &session_id, build_ref);

    execute_solve(
        driver,
        &session_id,
        solve_request,
        services,
        tear_down_handler,
        deadline,
    )
    .await
}

async fn validate_local_mounts(
    mounts: HashMap<String, PathBuf>,
) -> Result<HashMap<String, Arc<cap_std::fs::Dir>>, GrpcError> {
    let permit = filesystem_work_semaphore()
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| tonic::Status::internal("filesystem worker semaphore was closed"))?;

    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        mounts
            .into_iter()
            .map(|(name, path)| {
                if name.is_empty() {
                    return Err(GrpcError::InvalidLocalMount {
                        name,
                        path: path.display().to_string(),
                        reason: String::from("mount name must not be empty"),
                    });
                }

                let root = cap_std::fs::Dir::open_ambient_dir(&path, cap_std::ambient_authority())
                    .map_err(|error| GrpcError::InvalidLocalMount {
                        name: name.clone(),
                        path: path.display().to_string(),
                        reason: format!("cannot open directory root: {error}"),
                    })?;
                Ok((name, Arc::new(root)))
            })
            .collect()
    })
    .await
    .map_err(|error| tonic::Status::internal(format!("local mount validation failed: {error}")))?
}

fn filesystem_work_semaphore() -> &'static Arc<tokio::sync::Semaphore> {
    static SEMAPHORE: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SEMAPHORE.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(8)))
}

fn build_definition_solve_request(
    mut definition: bollard_buildkit_proto::pb::Definition,
    exporter: &DefinitionExporter,
    options: &DefinitionSolveOptions,
    session_id: &str,
    build_ref: Option<BuildRef>,
) -> SolveRequest {
    normalize_empty_source_locations(&mut definition);

    let (exporter_type, exporter_attrs, exporters) = match exporter {
        DefinitionExporter::Local(path) => {
            let mut attrs = HashMap::new();
            attrs.insert(String::from("dest"), path.to_string_lossy().to_string());
            let exporters = vec![Exporter {
                r#type: String::from("local"),
                attrs: attrs.clone(),
            }];
            (String::from("local"), attrs, exporters)
        }
    };

    SolveRequest {
        r#ref: build_ref.unwrap_or_default().into(),
        cache: Some(CacheOptions {
            export_ref_deprecated: String::new(),
            import_refs_deprecated: Vec::new(),
            export_attrs_deprecated: HashMap::new(),
            exports: options.cache_to.clone(),
            imports: options.cache_from.clone(),
        }),
        definition: Some(definition),
        entitlements: vec![],
        exporter_deprecated: exporter_type,
        exporter_attrs_deprecated: exporter_attrs,
        frontend: String::new(),
        frontend_attrs: HashMap::new(),
        frontend_inputs: HashMap::new(),
        session: String::from(session_id),
        exporters,
        internal: false,
        source_policy: None,
        enable_session_exporter: false,
        source_policy_session: String::new(),
    }
}

fn normalize_empty_source_locations(definition: &mut bollard_buildkit_proto::pb::Definition) {
    if let Some(source) = definition.source.as_mut() {
        source
            .locations
            .retain(|_, locations| !locations.locations.is_empty());
    }
}

async fn execute_solve<D: Driver>(
    driver: &D,
    session_id: &str,
    request: SolveRequest,
    services: Vec<GrpcServer>,
    tear_down_handler: Box<dyn DriverTearDownHandler>,
    deadline: Option<Instant>,
) -> Result<(), GrpcError> {
    let mut tear_down_guard = TearDownGuard::new(tear_down_handler);
    let mut control_client = match run_until(deadline, driver.grpc_handle(session_id, services))
        .await
    {
        Ok(client) => client
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE),
        Err(error) if is_deadline_exceeded(&error) => {
            drop(tear_down_guard);
            return Err(error);
        }
        Err(error) => {
            // A driver may have created resources before gRPC setup failed.
            if let Err(teardown_error) = tear_down_guard.tear_down().await {
                warn!("failed to tear down BuildKit driver after gRPC setup failure: {teardown_error}");
            }
            return Err(error);
        }
    };

    debug!(
        "sending solve request: ref={:?}, session={}, frontend={:?}, has_definition={}, exporter_count={}, cache_export_count={}, cache_import_count={}",
        request.r#ref,
        request.session,
        request.frontend,
        request.definition.is_some(),
        request.exporters.len(),
        request.cache.as_ref().map_or(0, |cache| cache.exports.len()),
        request.cache.as_ref().map_or(0, |cache| cache.imports.len()),
    );
    let solve_result = match run_until(
        deadline,
        control_client.solve(request).map_err(GrpcError::from),
    )
    .await
    {
        Ok(_) => Ok(()),
        Err(error) if is_deadline_exceeded(&error) => {
            drop(tear_down_guard);
            return Err(error);
        }
        Err(error) => Err(error),
    };
    debug!("solve res: {:#?}", solve_result);

    let tear_down_result = tear_down_guard.tear_down().await;

    // Preserve the original operation failure when cleanup also fails.
    match solve_result {
        Err(error) => {
            if let Err(teardown_error) = tear_down_result {
                warn!("failed to tear down BuildKit driver after solve failure: {teardown_error}");
            }
            Err(error)
        }
        Ok(_) => tear_down_result,
    }
}

async fn run_until<F, T>(deadline: Option<Instant>, future: F) -> Result<T, GrpcError>
where
    F: std::future::Future<Output = Result<T, GrpcError>>,
{
    match deadline {
        Some(deadline) => {
            let remaining = deadline.saturating_duration_since(Instant::now());
            tokio::time::timeout(remaining, future)
                .await
                .map_err(|_| GrpcError::from(tonic::Status::deadline_exceeded("solve timed out")))?
        }
        None => future.await,
    }
}

fn is_deadline_exceeded(error: &GrpcError) -> bool {
    matches!(
        error,
        GrpcError::TonicStatus { err } if err.code() == tonic::Code::DeadlineExceeded
    )
}

#[cfg(test)]
mod tests {
    use std::{
        net::SocketAddr,
        pin::Pin,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
    };

    use bollard_buildkit_proto::moby::buildkit::v1::{
        control_server::{Control, ControlServer},
        BuildHistoryEvent, BuildHistoryRequest, BytesMessage, DiskUsageRequest, DiskUsageResponse,
        InfoRequest, InfoResponse, ListWorkersRequest, ListWorkersResponse, PruneRequest,
        SolveResponse, StatusRequest, StatusResponse, UpdateBuildHistoryRequest,
        UpdateBuildHistoryResponse, UsageRecord,
    };
    use futures_util::{stream::Empty, FutureExt};
    use tempfile::tempdir;
    use tokio::{net::TcpListener, sync::oneshot};
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::{
        transport::{Channel, Endpoint, Server},
        Request, Response, Status,
    };

    use super::*;

    struct TestDriver {
        endpoint: Endpoint,
        setup_error: Option<Status>,
        setup_pending: bool,
        setup_panic: bool,
        session_ids: Arc<Mutex<Vec<String>>>,
        service_names: Arc<Mutex<Vec<String>>>,
    }

    impl Driver for TestDriver {
        async fn grpc_handle(
            &self,
            session_id: &str,
            _services: Vec<GrpcServer>,
        ) -> Result<ControlClient<InterceptedService<Channel, DriverInterceptor>>, GrpcError>
        {
            self.session_ids
                .lock()
                .expect("session IDs mutex is not poisoned")
                .push(String::from(session_id));
            self.service_names
                .lock()
                .expect("service names mutex is not poisoned")
                .extend(_services.iter().flat_map(GrpcServer::names));

            if let Some(error) = &self.setup_error {
                return Err(error.clone().into());
            }
            if self.setup_pending {
                std::future::pending::<()>().await;
            }
            if self.setup_panic {
                panic!("grpc setup panicked");
            }

            let interceptor = DriverInterceptor {
                session_id: String::from(session_id),
                metadata_grpc_method: Vec::new(),
            };

            let channel = self.endpoint.connect().await?;
            Ok(ControlClient::with_interceptor(channel, interceptor))
        }

        fn begin_solve(&self) -> Result<Box<dyn DriverTearDownHandler>, GrpcError> {
            Ok(Box::new(TestTearDown::default()))
        }
    }

    #[derive(Default)]
    struct TestTearDown {
        calls: Arc<AtomicUsize>,
        error: Option<Status>,
    }

    impl DriverTearDownHandler for TestTearDown {
        fn tear_down(
            &self,
        ) -> Pin<Box<dyn futures_core::Future<Output = Result<(), GrpcError>> + Send + 'static>>
        {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let result = self
                .error
                .clone()
                .map_or(Ok(()), |error| Err(GrpcError::from(error)));
            Box::pin(std::future::ready(result))
        }
    }

    struct TestControl {
        solve_error: Option<Status>,
    }

    type EmptyUsageRecords = Empty<Result<UsageRecord, Status>>;
    type EmptyStatusResponses = Empty<Result<StatusResponse, Status>>;
    type EmptyBytesMessages = Empty<Result<BytesMessage, Status>>;
    type EmptyBuildHistoryEvents = Empty<Result<BuildHistoryEvent, Status>>;

    #[tonic::async_trait]
    impl Control for TestControl {
        type PruneStream = EmptyUsageRecords;
        type StatusStream = EmptyStatusResponses;
        type SessionStream = EmptyBytesMessages;
        type ListenBuildHistoryStream = EmptyBuildHistoryEvents;

        async fn disk_usage(
            &self,
            _request: Request<DiskUsageRequest>,
        ) -> Result<Response<DiskUsageResponse>, Status> {
            Err(Status::unimplemented("test service"))
        }

        async fn prune(
            &self,
            _request: Request<PruneRequest>,
        ) -> Result<Response<Self::PruneStream>, Status> {
            Err(Status::unimplemented("test service"))
        }

        async fn solve(
            &self,
            _request: Request<SolveRequest>,
        ) -> Result<Response<SolveResponse>, Status> {
            match &self.solve_error {
                Some(error) => Err(error.clone()),
                None => Ok(Response::new(SolveResponse::default())),
            }
        }

        async fn status(
            &self,
            _request: Request<StatusRequest>,
        ) -> Result<Response<Self::StatusStream>, Status> {
            Err(Status::unimplemented("test service"))
        }

        async fn session(
            &self,
            _request: Request<tonic::Streaming<BytesMessage>>,
        ) -> Result<Response<Self::SessionStream>, Status> {
            Err(Status::unimplemented("test service"))
        }

        async fn list_workers(
            &self,
            _request: Request<ListWorkersRequest>,
        ) -> Result<Response<ListWorkersResponse>, Status> {
            Err(Status::unimplemented("test service"))
        }

        async fn info(
            &self,
            _request: Request<InfoRequest>,
        ) -> Result<Response<InfoResponse>, Status> {
            Err(Status::unimplemented("test service"))
        }

        async fn listen_build_history(
            &self,
            _request: Request<BuildHistoryRequest>,
        ) -> Result<Response<Self::ListenBuildHistoryStream>, Status> {
            Err(Status::unimplemented("test service"))
        }

        async fn update_build_history(
            &self,
            _request: Request<UpdateBuildHistoryRequest>,
        ) -> Result<Response<UpdateBuildHistoryResponse>, Status> {
            Err(Status::unimplemented("test service"))
        }
    }

    async fn start_test_server(
        solve_error: Option<Status>,
    ) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let server = Server::builder()
            .add_service(ControlServer::new(TestControl { solve_error }))
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                let _ = shutdown_receiver.await;
            });
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });

        (address, shutdown_sender, handle)
    }

    async fn stop_test_server(
        shutdown_sender: oneshot::Sender<()>,
        handle: tokio::task::JoinHandle<()>,
    ) {
        let _ = shutdown_sender.send(());
        handle.await.unwrap();
    }

    fn test_driver(address: SocketAddr) -> TestDriver {
        TestDriver {
            endpoint: Endpoint::from_shared(format!("http://{address}")).unwrap(),
            setup_error: None,
            setup_pending: false,
            setup_panic: false,
            session_ids: Arc::new(Mutex::new(Vec::new())),
            service_names: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn failing_test_driver() -> TestDriver {
        TestDriver {
            endpoint: Endpoint::from_static("http://127.0.0.1:1"),
            setup_error: Some(Status::internal("grpc setup failed")),
            setup_pending: false,
            setup_panic: false,
            session_ids: Arc::new(Mutex::new(Vec::new())),
            service_names: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn pending_test_driver() -> TestDriver {
        TestDriver {
            endpoint: Endpoint::from_static("http://127.0.0.1:1"),
            setup_error: None,
            setup_pending: true,
            setup_panic: false,
            session_ids: Arc::new(Mutex::new(Vec::new())),
            service_names: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn panicking_test_driver() -> TestDriver {
        TestDriver {
            endpoint: Endpoint::from_static("http://127.0.0.1:1"),
            setup_error: None,
            setup_pending: false,
            setup_panic: true,
            session_ids: Arc::new(Mutex::new(Vec::new())),
            service_names: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn teardown(calls: Arc<AtomicUsize>, error: Option<Status>) -> Box<dyn DriverTearDownHandler> {
        Box::new(TestTearDown { calls, error })
    }

    fn status_code(result: Result<(), GrpcError>) -> tonic::Code {
        match result {
            Err(GrpcError::TonicStatus { err }) => err.code(),
            other => panic!("expected tonic status error, got {other:?}"),
        }
    }

    #[test]
    fn definition_solve_request_has_definition_and_empty_frontend() {
        let request = build_definition_solve_request(
            bollard_buildkit_proto::pb::Definition::default(),
            &DefinitionExporter::Local(PathBuf::from("/out")),
            &DefinitionSolveOptions::default(),
            "session-id",
            None,
        );

        assert!(request.definition.is_some());
        assert!(request.frontend.is_empty());
        assert!(request.frontend_attrs.is_empty());
        assert!(request.frontend_inputs.is_empty());
        assert_eq!(request.exporter_deprecated, "local");
        assert_eq!(request.exporters.len(), 1);
        assert_eq!(request.exporters[0].r#type, "local");
        assert_eq!(
            request.exporters[0].attrs.get("dest"),
            Some(&String::from("/out"))
        );
        assert_eq!(request.session, "session-id");
    }

    #[test]
    fn definition_options_are_configured_through_builder() {
        let cache = bollard_buildkit_proto::moby::buildkit::v1::CacheOptionsEntry {
            r#type: String::from("local"),
            attrs: HashMap::new(),
        };
        let options = DefinitionSolveOptionsBuilder::new()
            .cache_to(&cache)
            .cache_from(&cache)
            .credential("registry.example", DockerCredentials::default())
            .secret("token", SecretSource::Env(String::from("TOKEN")))
            .enable_ssh(true)
            .timeout(Duration::from_secs(3))
            .file_transfer_limits(FileTransferLimits {
                max_files: Some(2),
                max_bytes: Some(8),
            })
            .build();

        assert_eq!(options.cache_to.len(), 1);
        assert_eq!(options.cache_from.len(), 1);
        assert!(options.credentials.contains_key("registry.example"));
        assert!(options.secrets.contains_key("token"));
        assert!(options.ssh);
        assert_eq!(options.timeout, Some(Duration::from_secs(3)));
        assert_eq!(options.file_transfer_limits.max_files, Some(2));
    }

    #[test]
    fn definition_options_store_local_mounts() {
        let path = PathBuf::from("/tmp/context");
        let options = DefinitionSolveOptionsBuilder::new()
            .local_mount("context", path.clone())
            .build();

        assert_eq!(options.local_mounts.get("context"), Some(&path));
    }

    #[tokio::test]
    async fn definition_solve_registers_shared_services_without_upload() {
        let driver = failing_test_driver();
        let service_names = Arc::clone(&driver.service_names);
        let request = DefinitionSolveRequest::new(
            bollard_buildkit_proto::pb::Definition::default(),
            DefinitionExporter::Local(PathBuf::from("/out")),
        )
        .with_options(
            DefinitionSolveOptionsBuilder::new()
                .enable_ssh(true)
                .build(),
        );

        assert!(solve_definition(&driver, request).await.is_err());
        let names = service_names.lock().unwrap();
        assert!(names.iter().any(|name| name.contains("credentials")));
        assert!(names.iter().any(|name| name.contains("GetSecret")));
        assert!(names.iter().any(|name| name.contains("ForwardAgent")));
        assert!(names.iter().any(|name| name.contains("diffcopy")));
        assert!(!names.iter().any(|name| name.contains("upload")));
    }

    #[tokio::test]
    async fn definition_solve_registers_filesync_for_local_mounts() {
        let root = tempdir().expect("temporary local mount exists");
        let driver = failing_test_driver();
        let service_names = Arc::clone(&driver.service_names);
        let request = DefinitionSolveRequest::new(
            bollard_buildkit_proto::pb::Definition::default(),
            DefinitionExporter::Local(PathBuf::from("/out")),
        )
        .with_options(
            DefinitionSolveOptionsBuilder::new()
                .local_mount("context", root.path())
                .build(),
        );

        assert!(solve_definition(&driver, request).await.is_err());
        let names = service_names.lock().unwrap();
        assert!(names
            .iter()
            .any(|name| name == "/moby.filesync.v1.FileSync/diffcopy"));
    }

    #[tokio::test]
    async fn definition_solve_rejects_invalid_local_mount_before_driver_setup() {
        let driver = failing_test_driver();
        let service_names = Arc::clone(&driver.service_names);
        let request = DefinitionSolveRequest::new(
            bollard_buildkit_proto::pb::Definition::default(),
            DefinitionExporter::Local(PathBuf::from("/out")),
        )
        .with_options(
            DefinitionSolveOptionsBuilder::new()
                .local_mount("context", "/path/that/does/not/exist")
                .build(),
        );

        let error = solve_definition(&driver, request)
            .await
            .expect_err("invalid local mount must fail");
        assert!(error.to_string().contains("invalid local mount"));
        assert!(service_names.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn execute_solve_tears_down_after_setup_failure() {
        let calls = Arc::new(AtomicUsize::new(0));
        let result = execute_solve(
            &failing_test_driver(),
            "session",
            SolveRequest::default(),
            Vec::new(),
            teardown(calls.clone(), Some(Status::aborted("teardown failed"))),
            None,
        )
        .await;

        assert_eq!(status_code(result), tonic::Code::Internal);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_solve_preserves_solve_failure_over_teardown_failure() {
        let (address, shutdown_sender, handle) =
            start_test_server(Some(Status::not_found("solve failed"))).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let result = execute_solve(
            &test_driver(address),
            "session",
            SolveRequest::default(),
            Vec::new(),
            teardown(calls.clone(), Some(Status::aborted("teardown failed"))),
            None,
        )
        .await;
        stop_test_server(shutdown_sender, handle).await;

        assert_eq!(status_code(result), tonic::Code::NotFound);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_solve_returns_teardown_failure_after_success() {
        let (address, shutdown_sender, handle) = start_test_server(None).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let result = execute_solve(
            &test_driver(address),
            "session",
            SolveRequest::default(),
            Vec::new(),
            teardown(calls.clone(), Some(Status::aborted("teardown failed"))),
            None,
        )
        .await;
        stop_test_server(shutdown_sender, handle).await;

        assert_eq!(status_code(result), tonic::Code::Aborted);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_solve_succeeds_and_tears_down_once() {
        let (address, shutdown_sender, handle) = start_test_server(None).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let result = execute_solve(
            &test_driver(address),
            "session",
            SolveRequest::default(),
            Vec::new(),
            teardown(calls.clone(), None),
            None,
        )
        .await;
        stop_test_server(shutdown_sender, handle).await;

        assert!(result.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn solve_reuses_a_driver_with_fresh_session_ids() {
        let (address, shutdown_sender, handle) = start_test_server(None).await;
        let driver = test_driver(address);
        let session_ids = Arc::clone(&driver.session_ids);

        for _ in 0..2 {
            solve(
                &driver,
                "moby",
                HashMap::new(),
                None,
                ImageBuildFrontendOptions::default(),
                ImageBuildLoadInput::Upload(bytes::Bytes::new()),
                None,
                None,
            )
            .await
            .unwrap();
        }

        stop_test_server(shutdown_sender, handle).await;

        let session_ids = session_ids
            .lock()
            .expect("session IDs mutex is not poisoned");
        assert_eq!(session_ids.len(), 2);
        assert_ne!(session_ids[0], session_ids[1]);
    }

    #[tokio::test]
    async fn solve_definition_reuses_a_driver_with_fresh_session_ids() {
        let (address, shutdown_sender, handle) = start_test_server(None).await;
        let driver = test_driver(address);
        let session_ids = Arc::clone(&driver.session_ids);

        for _ in 0..2 {
            let request = DefinitionSolveRequest::new(
                bollard_buildkit_proto::pb::Definition::default(),
                DefinitionExporter::Local(PathBuf::from("/out")),
            );
            solve_definition(&driver, request).await.unwrap();
        }

        stop_test_server(shutdown_sender, handle).await;
        let session_ids = session_ids.lock().unwrap();
        assert_eq!(session_ids.len(), 2);
        assert_ne!(session_ids[0], session_ids[1]);
    }

    #[tokio::test]
    async fn execute_solve_tears_down_after_cancellation() {
        let calls = Arc::new(AtomicUsize::new(0));
        let driver = pending_test_driver();
        let result = tokio::time::timeout(
            Duration::from_millis(10),
            execute_solve(
                &driver,
                "session",
                SolveRequest::default(),
                Vec::new(),
                teardown(calls.clone(), None),
                None,
            ),
        )
        .await;

        assert!(result.is_err());
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_solve_returns_deadline_and_detaches_teardown() {
        let calls = Arc::new(AtomicUsize::new(0));
        let result = execute_solve(
            &pending_test_driver(),
            "session",
            SolveRequest::default(),
            Vec::new(),
            teardown(calls.clone(), None),
            Some(Instant::now() + Duration::from_millis(1)),
        )
        .await;

        assert_eq!(status_code(result), tonic::Code::DeadlineExceeded);
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_solve_tears_down_after_panic() {
        let calls = Arc::new(AtomicUsize::new(0));
        let result = std::panic::AssertUnwindSafe(execute_solve(
            &panicking_test_driver(),
            "session",
            SolveRequest::default(),
            Vec::new(),
            teardown(calls.clone(), None),
            None,
        ))
        .catch_unwind()
        .await;

        assert!(result.is_err());
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
