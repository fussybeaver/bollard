use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use bollard_buildkit_proto::moby::{
    buildkit::{
        secrets::v1::secrets_server::SecretsServer,
        v1::{
            control_client::ControlClient, CacheOptions, CacheOptionsEntry, Exporter, SolveRequest,
        },
    },
    filesync::{
        packet::file_send_server::FileSendServer as FileSendPacketServer,
        v1::{auth_server::AuthServer, file_send_server::FileSendServer},
    },
    sshforward::v1::ssh_server::SshServer,
    upload::v1::upload_server::UploadServer,
};
use bollard_buildkit_proto::pb;
use log::debug;
// use tonic::service::Interceptor;
use tonic::{
    codegen::InterceptedService, metadata::MetadataValue, service::Interceptor, transport::Channel,
};

use crate::{
    auth::DockerCredentials,
    grpc::{build::ImageBuildFrontendOptionsIngest, build::SecretSource, BuildRef},
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
const DEFAULT_MAX_SEND_MSG_SIZE: usize = 16 << 20;
/// DEFAULT_MAX_RECV_MSG_SIZE defines the default maximum message size for
/// receiving protobufs passed over the GRPC API.
/// See https://github.com/containerd/containerd/blob/997f813b5cfdd7e120ee60d93b83ac6babbcfb1a/defaults/defaults.go#L20-L22
/// Used by buildkit [here](https://github.com/moby/buildkit/blob/082e8d8cf3267ddd3a28de1e258eaec20ebe3bbe/cmd/buildkitd/main.go#L309)
const DEFAULT_MAX_RECV_MSG_SIZE: usize = 16 << 20;

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
        self,
        session_id: &str,
        services: Vec<GrpcServer>,
    ) -> Result<ControlClient<InterceptedService<Channel, DriverInterceptor>>, GrpcError>;
    fn get_tear_down_handler(&self) -> Box<dyn DriverTearDownHandler>;
}

pub(crate) trait DriverTearDownHandler {
    fn tear_down<'a>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn futures_core::Future<Output = Result<(), GrpcError>> + 'a>>;
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
    /// Cache export entries.
    pub cache_to: Vec<CacheOptionsEntry>,
    /// Cache import entries.
    pub cache_from: Vec<CacheOptionsEntry>,
    /// Optional registry credentials for image pulls inside the graph.
    pub credentials: Option<HashMap<String, DockerCredentials>>,
    /// Secret sources exposed to the solve.
    pub secrets: HashMap<String, SecretSource>,
    /// Enable SSH agent forwarding.
    pub ssh: bool,
}

/// A direct-definition solve request.
#[derive(Debug, Clone)]
pub struct DefinitionSolveRequest {
    /// The pre-built LLB definition to solve.
    pub definition: pb::Definition,
    /// Where to export the result.
    pub exporter: DefinitionExporter,
    /// Solve options.
    pub options: DefinitionSolveOptions,
    /// Optional build reference.
    pub build_ref: Option<BuildRef>,
}

impl DefinitionSolveRequest {
    /// Construct a request for a definition and an exporter.
    pub fn new(definition: pb::Definition, exporter: DefinitionExporter) -> Self {
        Self {
            definition,
            exporter,
            options: DefinitionSolveOptions::default(),
            build_ref: None,
        }
    }

    /// Set solve options.
    pub fn with_options(mut self, options: DefinitionSolveOptions) -> Self {
        self.options = options;
        self
    }

    /// Set a build reference.
    pub fn with_build_ref(mut self, build_ref: BuildRef) -> Self {
        self.build_ref = Some(build_ref);
        self
    }
}

/// Trait enabling container exports.
pub trait Export {
    /// Export the container to a tar
    async fn export(
        self,
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
        self,
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
        self,
        output: ImageRegistryOutput,
        frontend_opts: ImageBuildFrontendOptions,
        load_input: ImageBuildLoadInput,
        credentials: Option<HashMap<&str, DockerCredentials>>,
        build_ref: Option<BuildRef>,
    ) -> Result<(), GrpcError>;
}

/// Trait enabling direct LLB definition solves.
pub trait SolveDefinition {
    /// Solve a pre-built LLB definition and export the result.
    async fn solve_definition(self, request: DefinitionSolveRequest) -> Result<(), GrpcError>;
}

#[allow(
    clippy::too_many_arguments,
    reason = "The nature of this function requires many parameters, maybe we can eventually create a Request structure?"
)]
pub(crate) async fn solve(
    driver: impl Driver,
    exporter: &str,
    exporter_attrs: BTreeMap<String, String>,
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

    let tear_down_handler = driver.get_tear_down_handler();
    let id = build_ref.unwrap_or_default();

    let solve_request = SolveRequest {
        r#ref: id.into(),
        cache: Some(CacheOptions {
            export_ref_deprecated: String::new(),
            import_refs_deprecated: Vec::new(),
            export_attrs_deprecated: BTreeMap::new(),
            exports: cache_to,
            imports: cache_from,
        }),
        definition: None,
        entitlements: vec![],
        exporter_deprecated: String::from(exporter),
        exporter_attrs_deprecated: exporter_attrs,
        frontend: String::from("dockerfile.v0"),
        frontend_attrs,
        frontend_inputs: BTreeMap::new(),
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
    )
    .await
}

async fn execute_solve<D: Driver>(
    driver: D,
    session_id: &str,
    request: SolveRequest,
    services: Vec<GrpcServer>,
    tear_down_handler: Box<dyn DriverTearDownHandler>,
) -> Result<(), GrpcError> {
    let client_result = driver.grpc_handle(session_id, services).await;
    let mut control_client = match client_result {
        Ok(client) => client
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE),
        Err(e) => {
            let _ = tear_down_handler.tear_down().await;
            return Err(e);
        }
    };

    debug!("sending solve request: {:#?}", request);
    let solve_result = control_client.solve(request).await;
    debug!("solve res: {:#?}", solve_result);

    let teardown_result = tear_down_handler.tear_down().await;

    solve_result?;
    teardown_result
}

pub(crate) async fn solve_definition(
    driver: impl Driver,
    request: DefinitionSolveRequest,
) -> Result<(), GrpcError> {
    let session_id = crate::grpc::new_id();

    let DefinitionSolveRequest {
        definition,
        exporter,
        options,
        build_ref,
    } = request;

    let mut auth_provider = super::AuthProvider::new();
    if let Some(creds) = options.credentials.clone() {
        for (host, docker_credentials) in creds {
            auth_provider.set_docker_credentials(&host, docker_credentials);
        }
    }

    let secret_provider = super::SecretProvider::new(options.secrets.clone());

    let auth = AuthServer::new(auth_provider);
    let secret = SecretsServer::new(secret_provider)
        .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
        .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);

    let mut services: Vec<GrpcServer> = vec![GrpcServer::Auth(auth), GrpcServer::Secrets(secret)];

    if options.ssh {
        let ssh_provider = super::SshProvider::new();
        let ssh = SshServer::new(ssh_provider)
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);

        services.push(GrpcServer::Ssh(ssh));
    }

    match &exporter {
        DefinitionExporter::Local(path) => {
            let filesend =
                FileSendPacketServer::new(super::FileSendPacketImpl::new(path.as_path()))
                    .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
                    .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);

            services.push(GrpcServer::FileSendPacket(filesend));
        }
    }

    let tear_down_handler = driver.get_tear_down_handler();
    let solve_request =
        build_definition_solve_request(definition, &exporter, &options, &session_id, build_ref);

    execute_solve(
        driver,
        &session_id,
        solve_request,
        services,
        tear_down_handler,
    )
    .await
}

fn build_definition_solve_request(
    definition: pb::Definition,
    exporter: &DefinitionExporter,
    options: &DefinitionSolveOptions,
    session_id: &str,
    build_ref: Option<BuildRef>,
) -> SolveRequest {
    let id = build_ref.unwrap_or_default();

    let (exporter_type, exporter_attrs, exporters) = match exporter {
        DefinitionExporter::Local(path) => {
            let mut attrs = BTreeMap::new();
            attrs.insert(String::from("dest"), path.to_string_lossy().to_string());
            let exporters = vec![Exporter {
                r#type: String::from("local"),
                attrs: attrs.clone(),
            }];
            ("local", attrs, exporters)
        }
    };

    SolveRequest {
        r#ref: id.into(),
        cache: Some(CacheOptions {
            export_ref_deprecated: String::new(),
            import_refs_deprecated: Vec::new(),
            export_attrs_deprecated: BTreeMap::new(),
            exports: options.cache_to.clone(),
            imports: options.cache_from.clone(),
        }),
        definition: Some(definition),
        entitlements: vec![],
        exporter_deprecated: String::from(exporter_type),
        exporter_attrs_deprecated: exporter_attrs,
        frontend: String::new(),
        frontend_attrs: BTreeMap::new(),
        frontend_inputs: BTreeMap::new(),
        session: String::from(session_id),
        exporters,
        internal: false,
        source_policy: None,
        enable_session_exporter: false,
        source_policy_session: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use bytes::Bytes;
    use tonic::transport::Channel;

    use super::*;

    struct CapturingDriver {
        captured: Arc<Mutex<Vec<String>>>,
    }

    impl Driver for CapturingDriver {
        async fn grpc_handle(
            self,
            _session_id: &str,
            services: Vec<GrpcServer>,
        ) -> Result<ControlClient<InterceptedService<Channel, DriverInterceptor>>, GrpcError>
        {
            let mut captured = self.captured.lock().unwrap();
            for service in &services {
                captured.extend(service.names());
            }
            Err(GrpcError::from(tonic::Status::internal("mock driver")))
        }

        fn get_tear_down_handler(&self) -> Box<dyn DriverTearDownHandler> {
            Box::new(NoopTearDownHandler)
        }
    }

    struct NoopTearDownHandler;

    impl DriverTearDownHandler for NoopTearDownHandler {
        fn tear_down<'a>(
            &'a self,
        ) -> Pin<Box<dyn futures_core::Future<Output = Result<(), GrpcError>> + 'a>> {
            Box::pin(futures_util::future::ok(()))
        }
    }

    struct RecordingTearDownHandler {
        called: Arc<Mutex<bool>>,
    }

    impl DriverTearDownHandler for RecordingTearDownHandler {
        fn tear_down<'a>(
            &'a self,
        ) -> Pin<Box<dyn futures_core::Future<Output = Result<(), GrpcError>> + 'a>> {
            let called = self.called.clone();
            Box::pin(async move {
                *called.lock().unwrap() = true;
                Ok(())
            })
        }
    }

    #[test]
    fn definition_solve_request_has_definition_and_empty_frontend() {
        let definition = pb::Definition::default();
        let request = build_definition_solve_request(
            definition,
            &DefinitionExporter::Local(PathBuf::from("/out")),
            &DefinitionSolveOptions::default(),
            "session-id",
            None,
        );

        assert!(request.definition.is_some());
        assert_eq!(request.frontend, "");
        assert!(request.frontend_attrs.is_empty());
        assert!(request.frontend_inputs.is_empty());
        assert_eq!(request.exporter_deprecated, "local");
        assert_eq!(request.exporters.len(), 1);
        assert_eq!(request.exporters[0].r#type, "local");
        assert_eq!(
            request.exporters[0].attrs.get("dest"),
            Some(&"/out".to_string())
        );
        assert_eq!(request.session, "session-id");
    }

    #[test]
    fn definition_solve_request_preserves_cache_options() {
        let mut options = DefinitionSolveOptions::default();
        options.cache_to.push(CacheOptionsEntry {
            r#type: String::from("local"),
            attrs: BTreeMap::new(),
        });
        options.cache_from.push(CacheOptionsEntry {
            r#type: String::from("registry"),
            attrs: BTreeMap::new(),
        });

        let request = build_definition_solve_request(
            pb::Definition::default(),
            &DefinitionExporter::Local(PathBuf::from("/out")),
            &options,
            "session-id",
            None,
        );

        let cache = request.cache.expect("cache options present");
        assert_eq!(cache.exports.len(), 1);
        assert_eq!(cache.exports[0].r#type, "local");
        assert_eq!(cache.imports.len(), 1);
        assert_eq!(cache.imports[0].r#type, "registry");
    }

    #[tokio::test]
    async fn definition_solve_registers_auth_secret_ssh_and_local_filesend() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let driver = CapturingDriver {
            captured: captured.clone(),
        };

        let options = DefinitionSolveOptions {
            ssh: true,
            ..Default::default()
        };
        let request = DefinitionSolveRequest::new(
            pb::Definition::default(),
            DefinitionExporter::Local(PathBuf::from("/out")),
        )
        .with_options(options);

        let result = solve_definition(driver, request).await;
        assert!(result.is_err());

        let names = captured.lock().unwrap();
        assert!(names.iter().any(|n| n.contains("Auth")));
        assert!(names.iter().any(|n| n.contains("Secrets")));
        assert!(names.iter().any(|n| n.contains("SSH")));
        assert!(names.iter().any(|n| n.contains("FileSend")));
    }

    #[tokio::test]
    async fn dockerfile_solve_registers_upload_and_no_filesend_without_path() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let driver = CapturingDriver {
            captured: captured.clone(),
        };

        let result = solve(
            driver,
            "oci",
            BTreeMap::new(),
            None,
            ImageBuildFrontendOptions::default(),
            ImageBuildLoadInput::Upload(Bytes::from_static(b"")),
            None,
            None,
        )
        .await;
        assert!(result.is_err());

        let names = captured.lock().unwrap();
        assert!(names.iter().any(|n| n.contains("Upload")));
        assert!(!names.iter().any(|n| n.contains("FileSend")));
    }

    #[tokio::test]
    async fn execute_solve_tears_down_when_grpc_handle_fails() {
        let called = Arc::new(Mutex::new(false));
        let handler = RecordingTearDownHandler {
            called: called.clone(),
        };
        let driver = CapturingDriver {
            captured: Arc::new(Mutex::new(Vec::new())),
        };

        let request = SolveRequest::default();
        let result = execute_solve(driver, "session", request, vec![], Box::new(handler)).await;
        assert!(result.is_err());
        assert!(*called.lock().unwrap());
    }
}
