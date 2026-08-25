use std::time::Duration;
use std::{collections::HashMap, fmt, path::PathBuf, sync::Arc, time::Instant};

use bollard_buildkit_proto::moby::{
    buildkit::{
        secrets::v1::secrets_server::SecretsServer,
        v1::{control_client::ControlClient, CacheOptions, Exporter, SolveRequest},
    },
    filesync::{
        packet::file_send_server::FileSendServer as FileSendPacketServer,
        v1::{
            auth_server::AuthServer, file_send_server::FileSendServer,
            file_sync_server::FileSyncServer,
        },
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
const DEFAULT_DEFINITION_SOLVE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

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

/// Cleans up driver resources created for a solve.
///
/// Implementations must be idempotent: when a solve future is cancelled while
/// teardown is in flight, the guard retries teardown once from a fresh
/// runtime, so `tear_down` may observe partially torn-down state or run more
/// than once for the same solve.
pub(crate) trait DriverTearDownHandler: Send + Sync {
    fn tear_down(
        &self,
    ) -> std::pin::Pin<Box<dyn futures_core::Future<Output = Result<(), GrpcError>> + Send + 'static>>;
}

struct TearDownGuard {
    handler: Arc<dyn DriverTearDownHandler>,
    timeout: Duration,
    armed: bool,
    completed: bool,
}

impl TearDownGuard {
    fn new(handler: Box<dyn DriverTearDownHandler>) -> Self {
        Self::with_timeout(handler, TEAR_DOWN_TIMEOUT)
    }

    fn with_timeout(handler: Box<dyn DriverTearDownHandler>, timeout: Duration) -> Self {
        Self {
            handler: Arc::from(handler),
            timeout,
            armed: true,
            completed: false,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    async fn tear_down(&mut self) -> Result<(), GrpcError> {
        if self.completed {
            return Err(GrpcError::TearDownTaskUnavailable);
        }
        let result = run_tear_down(Arc::clone(&self.handler), self.timeout).await;
        self.completed = true;
        result
    }
}

async fn run_tear_down(
    handler: Arc<dyn DriverTearDownHandler>,
    timeout: Duration,
) -> Result<(), GrpcError> {
    match tokio::time::timeout(timeout, handler.tear_down()).await {
        Ok(result) => result,
        Err(_) => Err(GrpcError::from(tonic::Status::deadline_exceeded(
            "driver teardown exceeded its timeout",
        ))),
    }
}

impl Drop for TearDownGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }

        if self.completed {
            return;
        }

        let handler = Arc::clone(&self.handler);
        let timeout = self.timeout;
        let thread = std::thread::Builder::new()
            .name(String::from("bollard-buildkit-teardown"))
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match runtime {
                    Ok(runtime) => {
                        if let Err(error) = runtime.block_on(run_tear_down(handler, timeout)) {
                            warn!(
                                "failed to tear down BuildKit driver after cancellation: {error}"
                            );
                        }
                    }
                    Err(error) => {
                        warn!("failed to create BuildKit teardown runtime: {error}");
                    }
                }
            });
        if let Err(error) = thread {
            warn!("failed to spawn BuildKit teardown thread: {error}");
        }
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
#[derive(Clone)]
#[non_exhaustive]
pub enum DefinitionExporter {
    /// Export the solved filesystem into a local directory.
    Local(PathBuf),
}

impl std::fmt::Debug for DefinitionExporter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => formatter.write_str("DefinitionExporter::Local(..)"),
        }
    }
}

/// Options for a direct LLB definition solve.
#[derive(Clone)]
pub struct DefinitionSolveOptions {
    cache_to: Vec<bollard_buildkit_proto::moby::buildkit::v1::CacheOptionsEntry>,
    cache_from: Vec<bollard_buildkit_proto::moby::buildkit::v1::CacheOptionsEntry>,
    credentials: HashMap<String, DockerCredentials>,
    secrets: HashMap<String, SecretSource>,
    ssh: bool,
    timeout: Option<Duration>,
    file_transfer_limits: FileTransferLimits,
    local_mounts: HashMap<String, LocalMount>,
}

impl std::fmt::Debug for DefinitionSolveOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DefinitionSolveOptions")
            .field("cache_to_count", &self.cache_to.len())
            .field("cache_from_count", &self.cache_from.len())
            .field("credential_count", &self.credentials.len())
            .field("secret_count", &self.secrets.len())
            .field("ssh", &self.ssh)
            .field("timeout", &self.timeout)
            .field("file_transfer_limits", &self.file_transfer_limits)
            .field("local_mount_count", &self.local_mounts.len())
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct LocalMount {
    pub(crate) root: Arc<cap_std::fs::Dir>,
    pub(crate) path: PathBuf,
}

impl std::fmt::Debug for LocalMount {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalMount")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Default for DefinitionSolveOptions {
    fn default() -> Self {
        Self {
            cache_to: Vec::new(),
            cache_from: Vec::new(),
            credentials: HashMap::new(),
            secrets: HashMap::new(),
            ssh: false,
            timeout: Some(DEFAULT_DEFINITION_SOLVE_TIMEOUT),
            file_transfer_limits: FileTransferLimits::default(),
            local_mounts: HashMap::new(),
        }
    }
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
    ///
    /// The default is ten minutes. Teardown has a separate bounded timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.options.timeout = Some(timeout);
        self
    }

    /// Reject a packet export once it reaches this many filesystem entries.
    pub fn max_files(mut self, max_files: u64) -> Self {
        self.options.file_transfer_limits.max_files = Some(max_files);
        self
    }

    /// Reject a packet export once its declared entry sizes exceed this total.
    pub fn max_bytes(mut self, max_bytes: u64) -> Self {
        self.options.file_transfer_limits.max_bytes = Some(max_bytes);
        self
    }

    /// Expose a host directory under a BuildKit `local://` source name.
    ///
    /// On Windows, local-source paths remain subject to the host's long-path
    /// configuration and the underlying filesystem runtime. Enable Windows
    /// long-path support when exposing deeply nested directories.
    pub fn local_mount(
        mut self,
        name: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<Self, GrpcError> {
        let name = name.into();
        let path = path.into();
        if name.is_empty() {
            return Err(GrpcError::InvalidLocalMount {
                name,
                path,
                reason: String::from("mount name must not be empty"),
            });
        }

        let root = cap_std::fs::Dir::open_ambient_dir(&path, cap_std::ambient_authority())
            .map_err(|error| GrpcError::InvalidLocalMount {
                name: name.clone(),
                path: path.clone(),
                reason: error.to_string(),
            })?;

        self.options.local_mounts.insert(
            name,
            LocalMount {
                root: Arc::new(root),
                path,
            },
        );
        Ok(self)
    }

    /// Consume the builder and return immutable solve options.
    pub fn build(self) -> DefinitionSolveOptions {
        self.options
    }
}

/// A direct-definition solve request.
#[derive(Clone)]
pub struct DefinitionSolveRequest {
    /// The pre-built LLB definition to solve.
    pub definition: bollard_buildkit_proto::pb::Definition,
    /// Where to export the result.
    pub exporter: DefinitionExporter,
    options: DefinitionSolveOptions,
    build_ref: Option<BuildRef>,
}

impl fmt::Debug for DefinitionSolveRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let exporter = match &self.exporter {
            DefinitionExporter::Local(_) => "local",
        };

        formatter
            .debug_struct("DefinitionSolveRequest")
            .field("definition_ops", &self.definition.def.len())
            .field("exporter", &exporter)
            .field("options", &self.options)
            .field("has_build_ref", &self.build_ref.is_some())
            .finish()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct SolveRequestSummary {
    has_ref: bool,
    has_session: bool,
    has_definition: bool,
    definition_ops: usize,
    frontend_attrs: usize,
    frontend_inputs: usize,
    entitlements: usize,
    exporters: usize,
    cache_exports: usize,
    cache_imports: usize,
    has_source_policy: bool,
    has_source_policy_session: bool,
    internal: bool,
    enable_session_exporter: bool,
}

impl From<&SolveRequest> for SolveRequestSummary {
    fn from(request: &SolveRequest) -> Self {
        Self {
            has_ref: !request.r#ref.is_empty(),
            has_session: !request.session.is_empty(),
            has_definition: request.definition.is_some(),
            definition_ops: request
                .definition
                .as_ref()
                .map_or(0, |definition| definition.def.len()),
            frontend_attrs: request.frontend_attrs.len(),
            frontend_inputs: request.frontend_inputs.len(),
            entitlements: request.entitlements.len(),
            exporters: request.exporters.len(),
            cache_exports: request
                .cache
                .as_ref()
                .map_or(0, |cache| cache.exports.len()),
            cache_imports: request
                .cache
                .as_ref()
                .map_or(0, |cache| cache.imports.len()),
            has_source_policy: request.source_policy.is_some(),
            has_source_policy_session: !request.source_policy_session.is_empty(),
            internal: request.internal,
            enable_session_exporter: request.enable_session_exporter,
        }
    }
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
        // `ImageBuildFrontendOptions::enable_ssh` is still a plain bool, so
        // the GRPC-driver path keeps exactly its previous behaviour: the one
        // implicit `default` agent, forwarded to the host's `SSH_AUTH_SOCK`.
        // Named agents are exposed on `ImageBuildSessionProviders` only.
        let ssh_provider = super::SshProvider::new(HashMap::from([(
            String::from(super::DEFAULT_SSH_AGENT_ID),
            super::SshAgentSource::DefaultAgentSocket,
        )]));
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
        .map(|timeout| {
            Instant::now().checked_add(timeout).ok_or_else(|| {
                GrpcError::from(tonic::Status::invalid_argument(
                    "direct solve timeout is too large",
                ))
            })
        })
        .transpose()?;

    let DefinitionSolveRequest {
        definition,
        exporter,
        options,
        build_ref,
    } = request;

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
        let ssh = SshServer::new(super::SshProvider::new(HashMap::from([(
            String::from(super::DEFAULT_SSH_AGENT_ID),
            super::SshAgentSource::DefaultAgentSocket,
        )])))
        .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
        .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);
        services.push(GrpcServer::Ssh(ssh));
    }

    if !options.local_mounts.is_empty() {
        let mounts = options
            .local_mounts
            .iter()
            .map(|(name, mount)| (name.clone(), Arc::clone(&mount.root)))
            .collect();
        let filesync = FileSyncServer::new(super::filesync::FileSyncImpl::new(mounts))
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);
        services.push(GrpcServer::FileSync(filesync));
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

fn build_definition_solve_request(
    mut definition: bollard_buildkit_proto::pb::Definition,
    exporter: &DefinitionExporter,
    options: &DefinitionSolveOptions,
    session_id: &str,
    build_ref: Option<BuildRef>,
) -> SolveRequest {
    normalize_empty_source_locations(&mut definition);

    let (exporter_type, exporter_attrs, exporters) = match exporter {
        DefinitionExporter::Local(_) => {
            let exporters = vec![Exporter {
                r#type: String::from("local"),
                attrs: HashMap::new(),
            }];
            (String::from("local"), HashMap::new(), exporters)
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
    execute_solve_with_teardown_timeout(
        driver,
        session_id,
        request,
        services,
        tear_down_handler,
        deadline,
        TEAR_DOWN_TIMEOUT,
    )
    .await
}

async fn execute_solve_with_teardown_timeout<D: Driver>(
    driver: &D,
    session_id: &str,
    request: SolveRequest,
    services: Vec<GrpcServer>,
    tear_down_handler: Box<dyn DriverTearDownHandler>,
    deadline: Option<Instant>,
    teardown_timeout: Duration,
) -> Result<(), GrpcError> {
    let mut tear_down_guard = TearDownGuard::with_timeout(tear_down_handler, teardown_timeout);
    let mut control_client = match run_until(deadline, driver.grpc_handle(session_id, services))
        .await
    {
        Ok(client) => client
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE),
        Err(error) => {
            // A driver may have created resources before gRPC setup failed.
            if let Err(teardown_error) = tear_down_guard.tear_down().await {
                warn!("failed to tear down BuildKit driver after gRPC setup failure: {teardown_error}");
            }
            return Err(error);
        }
    };

    debug!(
        "sending solve request: {:?}",
        SolveRequestSummary::from(&request)
    );
    let solve_result = match run_until(
        deadline,
        control_client.solve(request).map_err(GrpcError::from),
    )
    .await
    {
        Ok(_) => Ok(()),
        Err(error) => Err(error),
    };
    debug!("solve completed: success={}", solve_result.is_ok());

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

#[cfg(test)]
mod tests {
    use std::{
        net::SocketAddr,
        pin::Pin,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
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
    use tokio::{
        net::TcpListener,
        sync::{oneshot, Notify},
    };
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
        started: Option<Arc<Notify>>,
    }

    impl DriverTearDownHandler for TestTearDown {
        fn tear_down(
            &self,
        ) -> Pin<Box<dyn futures_core::Future<Output = Result<(), GrpcError>> + Send + 'static>>
        {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(started) = &self.started {
                started.notify_one();
            }
            let result = self
                .error
                .clone()
                .map_or(Ok(()), |error| Err(GrpcError::from(error)));
            Box::pin(std::future::ready(result))
        }
    }

    struct BlockingTearDown {
        calls: Arc<AtomicUsize>,
        started: Arc<Notify>,
        cancelled: Arc<AtomicBool>,
    }

    impl DriverTearDownHandler for BlockingTearDown {
        fn tear_down(
            &self,
        ) -> Pin<Box<dyn futures_core::Future<Output = Result<(), GrpcError>> + Send + 'static>>
        {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.started.notify_one();
            let cancelled = Arc::clone(&self.cancelled);

            struct CancellationFlag(Arc<AtomicBool>);

            impl Drop for CancellationFlag {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::SeqCst);
                }
            }

            Box::pin(async move {
                let _cancellation_flag = CancellationFlag(cancelled);
                std::future::pending::<()>().await;
                #[allow(unreachable_code)]
                Ok(())
            })
        }
    }

    struct TestControl {
        solve_error: Option<Status>,
        solve_pending: bool,
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
            if self.solve_pending {
                std::future::pending::<()>().await;
            }
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
        start_test_server_with_pending(solve_error, false).await
    }

    async fn start_pending_solve_server(
    ) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
        start_test_server_with_pending(None, true).await
    }

    async fn start_test_server_with_pending(
        solve_error: Option<Status>,
        solve_pending: bool,
    ) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let server = Server::builder()
            .add_service(ControlServer::new(TestControl {
                solve_error,
                solve_pending,
            }))
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
        Box::new(TestTearDown {
            calls,
            error,
            started: None,
        })
    }

    fn teardown_with_started(
        calls: Arc<AtomicUsize>,
        started: Arc<Notify>,
    ) -> Box<dyn DriverTearDownHandler> {
        Box::new(TestTearDown {
            calls,
            error: None,
            started: Some(started),
        })
    }

    fn blocking_teardown(
        calls: Arc<AtomicUsize>,
        started: Arc<Notify>,
        cancelled: Arc<AtomicBool>,
    ) -> Box<dyn DriverTearDownHandler> {
        Box::new(BlockingTearDown {
            calls,
            started,
            cancelled,
        })
    }

    fn status_code(result: Result<(), GrpcError>) -> tonic::Code {
        match result {
            Err(GrpcError::TonicStatus { err }) => err.code(),
            other => panic!("expected tonic status error, got {other:?}"),
        }
    }

    async fn assert_teardown_timeout_case(
        driver: &TestDriver,
        deadline: Option<Instant>,
        expected_code: tonic::Code,
    ) {
        let calls = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let cancelled = Arc::new(AtomicBool::new(false));
        let result = execute_solve_with_teardown_timeout(
            driver,
            "session",
            SolveRequest::default(),
            Vec::new(),
            blocking_teardown(calls.clone(), started, cancelled.clone()),
            deadline,
            Duration::from_millis(5),
        )
        .await;

        assert_eq!(status_code(result), expected_code);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(cancelled.load(Ordering::SeqCst));
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
        assert!(request.exporter_attrs_deprecated.is_empty());
        assert_eq!(request.exporters.len(), 1);
        assert_eq!(request.exporters[0].r#type, "local");
        assert!(request.exporters[0].attrs.is_empty());
        assert!(!format!("{request:?}").contains("/out"));
        assert_eq!(request.session, "session-id");
    }

    #[test]
    fn solve_request_summary_redacts_request_values() {
        let mut request = SolveRequest {
            r#ref: String::from("sensitive-build-reference"),
            session: String::from("sensitive-session-id"),
            definition: Some(bollard_buildkit_proto::pb::Definition {
                def: vec![b"sensitive-definition-bytes".to_vec()],
                ..Default::default()
            }),
            ..Default::default()
        };
        request
            .frontend_attrs
            .insert(String::from("secret-path"), String::from("/private"));

        let summary = SolveRequestSummary::from(&request);
        let rendered = format!("{summary:?}");

        assert_eq!(summary.definition_ops, 1);
        assert_eq!(summary.frontend_attrs, 1);
        assert!(!rendered.contains("sensitive-build-reference"));
        assert!(!rendered.contains("sensitive-session-id"));
        assert!(!rendered.contains("sensitive-definition-bytes"));
        assert!(!rendered.contains("secret-path"));
        assert!(!rendered.contains("/private"));
    }

    #[test]
    fn definition_solve_debug_output_redacts_sensitive_values() {
        let builder = DefinitionSolveOptionsBuilder::new()
            .credential(
                "sensitive-registry.example",
                DockerCredentials {
                    username: Some(String::from("sensitive-username")),
                    password: Some(String::from("sensitive-password")),
                    auth: Some(String::from("sensitive-auth")),
                    identitytoken: Some(String::from("sensitive-identity-token")),
                    registrytoken: Some(String::from("sensitive-registry-token")),
                    ..Default::default()
                },
            )
            .secret(
                "sensitive-secret-id",
                SecretSource::Env(String::from("sensitive-secret-source")),
            );
        let builder_debug = format!("{builder:?}");
        let options = builder.clone().build();
        let options_debug = format!("{options:?}");
        let request = DefinitionSolveRequest::new(
            bollard_buildkit_proto::pb::Definition {
                def: vec![b"sensitive-definition-bytes".to_vec()],
                metadata: [(String::from("private-metadata"), Default::default())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            DefinitionExporter::Local(PathBuf::from("/sensitive-export-path")),
        )
        .with_options(options);
        let request_debug = format!("{request:?}");

        for rendered in [&builder_debug, &options_debug, &request_debug] {
            assert!(!rendered.contains("sensitive-registry.example"));
            assert!(!rendered.contains("sensitive-username"));
            assert!(!rendered.contains("sensitive-password"));
            assert!(!rendered.contains("sensitive-auth"));
            assert!(!rendered.contains("sensitive-identity-token"));
            assert!(!rendered.contains("sensitive-registry-token"));
            assert!(!rendered.contains("sensitive-secret-id"));
            assert!(!rendered.contains("sensitive-secret-source"));
            assert!(!rendered.contains("sensitive-definition-bytes"));
            assert!(!rendered.contains("private-metadata"));
            assert!(!rendered.contains("/sensitive-export-path"));
        }

        assert!(builder_debug.contains("credential_count: 1"));
        assert!(builder_debug.contains("secret_count: 1"));
        assert!(options_debug.contains("credential_count: 1"));
        assert!(options_debug.contains("secret_count: 1"));
        assert!(request_debug.contains("definition_ops: 1"));
        assert!(request_debug.contains("exporter: \"local\""));
        assert!(request_debug.contains("has_build_ref: false"));
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
            .max_files(2)
            .max_bytes(8)
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
    fn definition_options_open_and_retain_local_mount_capabilities() {
        let root = tempdir().expect("temporary local mount exists");
        let path = root.path().to_path_buf();
        let options = DefinitionSolveOptionsBuilder::new()
            .local_mount("context", &path)
            .expect("local mount opens")
            .build();

        let mount = options
            .local_mounts
            .get("context")
            .expect("local mount is stored");
        assert_eq!(mount.path, path);
        assert!(mount.root.open_dir(".").is_ok());
    }

    #[test]
    fn local_mount_rejects_empty_missing_and_non_directory_paths() {
        let root = tempdir().expect("temporary local mount exists");
        let file = root.path().join("file");
        std::fs::write(&file, b"not a directory").expect("temporary file is created");

        let empty_name = DefinitionSolveOptionsBuilder::new()
            .local_mount("", root.path())
            .expect_err("empty mount names are rejected");
        assert!(matches!(empty_name, GrpcError::InvalidLocalMount { .. }));

        let missing_path = DefinitionSolveOptionsBuilder::new()
            .local_mount("context", root.path().join("missing"))
            .expect_err("missing mount paths are rejected");
        assert!(matches!(missing_path, GrpcError::InvalidLocalMount { .. }));

        let regular_file = DefinitionSolveOptionsBuilder::new()
            .local_mount("context", &file)
            .expect_err("regular files are rejected as mount roots");
        assert!(matches!(regular_file, GrpcError::InvalidLocalMount { .. }));
    }

    #[test]
    fn definition_options_have_a_bounded_default_timeout() {
        assert_eq!(
            DefinitionSolveOptions::default().timeout,
            Some(DEFAULT_DEFINITION_SOLVE_TIMEOUT)
        );
    }

    #[tokio::test]
    async fn definition_solve_rejects_an_overflowing_timeout() {
        let request = DefinitionSolveRequest::new(
            bollard_buildkit_proto::pb::Definition::default(),
            DefinitionExporter::Local(PathBuf::from("/out")),
        )
        .with_options(
            DefinitionSolveOptionsBuilder::new()
                .timeout(Duration::MAX)
                .build(),
        );

        let error = solve_definition(&failing_test_driver(), request)
            .await
            .expect_err("an overflowing timeout must be rejected");
        assert!(matches!(
            error,
            GrpcError::TonicStatus { err } if err.code() == tonic::Code::InvalidArgument
        ));
    }

    #[tokio::test]
    async fn definition_solve_registers_expected_services() {
        for with_local_mount in [false, true] {
            let root = tempdir().expect("temporary local mount exists");
            let driver = failing_test_driver();
            let service_names = Arc::clone(&driver.service_names);
            let mut options = DefinitionSolveOptionsBuilder::new().enable_ssh(true);
            if with_local_mount {
                options = options
                    .local_mount("context", root.path())
                    .expect("local mount opens");
            }
            let request = DefinitionSolveRequest::new(
                bollard_buildkit_proto::pb::Definition::default(),
                DefinitionExporter::Local(PathBuf::from("/out")),
            )
            .with_options(options.build());

            assert!(solve_definition(&driver, request).await.is_err());
            let names = service_names.lock().unwrap();
            assert!(names.iter().any(|name| name.contains("credentials")));
            assert!(names.iter().any(|name| name.contains("GetSecret")));
            assert!(names.iter().any(|name| name.contains("ForwardAgent")));
            assert!(names.iter().any(|name| name.contains("diffcopy")));
            assert!(!names.iter().any(|name| name.contains("upload")));
            assert_eq!(
                names
                    .iter()
                    .filter(|name| name.as_str() == "/moby.filesync.v1.FileSync/diffcopy")
                    .count(),
                usize::from(with_local_mount)
            );
            assert!(!names
                .iter()
                .any(|name| name == "/moby.filesync.v1.FileSync/TarStream"));
        }
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
    async fn solve_entry_points_use_fresh_session_ids() {
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

        for _ in 0..2 {
            let request = DefinitionSolveRequest::new(
                bollard_buildkit_proto::pb::Definition::default(),
                DefinitionExporter::Local(PathBuf::from("/out")),
            );
            solve_definition(&driver, request).await.unwrap();
        }

        stop_test_server(shutdown_sender, handle).await;

        let session_ids = session_ids
            .lock()
            .expect("session IDs mutex is not poisoned");
        assert_eq!(session_ids.len(), 4);
        assert_ne!(session_ids[0], session_ids[1]);
        assert_ne!(session_ids[2], session_ids[3]);
        assert!(session_ids[..2]
            .iter()
            .all(|session_id| !session_ids[2..].contains(session_id)));
    }

    #[test]
    fn teardown_guard_drop_survives_runtime_shutdown() {
        let calls = Arc::new(AtomicUsize::new(0));
        let guard = {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                TearDownGuard::new(Box::new(TestTearDown {
                    calls: Arc::clone(&calls),
                    error: None,
                    started: None,
                }))
            })
        };

        drop(guard);
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while std::time::Instant::now() < deadline {
            if calls.load(Ordering::SeqCst) == 1 {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("teardown cleanup did not start after runtime shutdown");
    }

    #[tokio::test]
    async fn teardown_guard_retries_cleanup_cancelled_in_progress() {
        let calls = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let guard = TearDownGuard::with_timeout(
            Box::new(BlockingTearDown {
                calls: Arc::clone(&calls),
                started: Arc::clone(&started),
                cancelled: Arc::new(AtomicBool::new(false)),
            }),
            Duration::from_millis(10),
        );
        let task = tokio::spawn(async move {
            let mut guard = guard;
            let _ = guard.tear_down().await;
        });

        tokio::time::timeout(Duration::from_secs(1), started.notified())
            .await
            .expect("initial teardown starts");
        task.abort();
        let _ = task.await;

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if calls.load(Ordering::SeqCst) == 2 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelled teardown is retried once");
    }

    #[tokio::test]
    async fn execute_solve_tears_down_after_cancellation() {
        let calls = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let notification = started.notified();
        let driver = pending_test_driver();
        let result = tokio::time::timeout(
            Duration::from_millis(10),
            execute_solve(
                &driver,
                "session",
                SolveRequest::default(),
                Vec::new(),
                teardown_with_started(calls.clone(), started.clone()),
                None,
            ),
        )
        .await;

        assert!(result.is_err());
        tokio::time::timeout(Duration::from_secs(1), notification)
            .await
            .expect("cancelled solve starts teardown");

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_solve_awaits_teardown_after_setup_timeout() {
        let driver = pending_test_driver();
        assert_teardown_timeout_case(
            &driver,
            Some(Instant::now() + Duration::from_millis(1)),
            tonic::Code::DeadlineExceeded,
        )
        .await;
    }

    #[tokio::test]
    async fn execute_solve_awaits_teardown_after_solve_timeout() {
        let (address, shutdown_sender, handle) = start_pending_solve_server().await;
        let driver = test_driver(address);
        assert_teardown_timeout_case(
            &driver,
            Some(Instant::now() + Duration::from_millis(25)),
            tonic::Code::DeadlineExceeded,
        )
        .await;
        stop_test_server(shutdown_sender, handle).await;
    }

    #[tokio::test]
    async fn execute_solve_preserves_solve_error_over_teardown_timeout() {
        let (address, shutdown_sender, handle) =
            start_test_server(Some(Status::not_found("solve failed"))).await;
        let driver = test_driver(address);
        assert_teardown_timeout_case(&driver, None, tonic::Code::NotFound).await;
        stop_test_server(shutdown_sender, handle).await;
    }

    #[tokio::test]
    async fn execute_solve_tears_down_after_panic() {
        let calls = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let notification = started.notified();
        let result = std::panic::AssertUnwindSafe(execute_solve(
            &panicking_test_driver(),
            "session",
            SolveRequest::default(),
            Vec::new(),
            teardown_with_started(calls.clone(), started.clone()),
            None,
        ))
        .catch_unwind()
        .await;

        assert!(result.is_err());
        tokio::time::timeout(Duration::from_secs(1), notification)
            .await
            .expect("panicked solve starts teardown");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
