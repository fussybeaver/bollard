use std::pin::Pin;

use bollard_buildkit_proto::health::health_server::HealthServer;
pub use bollard_buildkit_proto::moby::buildkit::v1::control_client::ControlClient;

use crate::grpc::driver::ImageBuildFrontendOptions;
use crate::grpc::driver::ImageBuildLoadInput;
use crate::grpc::driver::ImageExporterEnum;

use crate::grpc::registry::ImageRegistryOutput;
use crate::grpc::DockerCredentials;
use futures_util::TryStreamExt;
use log::error;
use log::trace;
use std::collections::HashMap;
pub use tonic::transport::Endpoint;
use tonic::{service::interceptor::InterceptedService, transport::Channel};

use crate::grpc::io::into_async_read::IntoAsyncRead;
use crate::grpc::io::reader_stream::ReaderStream;
use crate::grpc::GrpcTransport;
use crate::grpc::{error::GrpcError, HealthServerImpl};

use super::{Driver, DriverInterceptor, DriverTearDownHandler};

const DUPLEX_BUF_SIZE: usize = 8 * 1024;

/// BuildkitDaemon is a client for the buildkit daemon.
#[derive(Debug, Clone)]
pub struct BuildkitDaemon {
    uri: Endpoint,
}

impl BuildkitDaemon {
    /// Create a [`BuildkitDaemon`] driver instance.
    pub fn new(uri: Endpoint) -> Self {
        Self { uri }
    }
}

impl Driver for BuildkitDaemon {
    async fn grpc_handle(
        self,
        session_id: &str,
        services: Vec<super::GrpcServer>,
    ) -> Result<ControlClient<InterceptedService<Channel, DriverInterceptor>>, GrpcError> {
        let channel = self.uri.connect().await?;
        let metadata_grpc_method: Vec<String> = services.iter().flat_map(|s| s.names()).collect();

        let interceptor = DriverInterceptor {
            session_id: String::from(session_id),
            metadata_grpc_method: metadata_grpc_method.clone(),
        };

        let mut control_client = ControlClient::with_interceptor(channel, interceptor);

        let (asyncwriter, asyncreader) = tokio::io::duplex(DUPLEX_BUF_SIZE);
        let streamreader = ReaderStream::new(asyncreader);
        let stream = control_client.session(streamreader).await?;
        let stream = stream
            .into_inner()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));

        let asyncreader = IntoAsyncRead::new(stream);
        let transport = GrpcTransport {
            read: Box::pin(asyncreader),
            write: Box::pin(asyncwriter),
        };

        tokio::spawn(async {
            let health = HealthServer::new(HealthServerImpl::new());
            let mut builder = tonic::transport::Server::builder();
            let mut router = builder.add_service(health);
            for service in services {
                router = service.append(router);
            }
            trace!("router: {:#?}", router);
            if let Err(e) = router
                .serve_with_incoming(futures_util::stream::iter(vec![Ok::<
                    _,
                    tonic::transport::Error,
                >(
                    transport
                )]))
                .await
            {
                error!("Failed to serve grpc connection: {}", e)
            }
        });

        Ok(control_client)
    }

    fn get_tear_down_handler(&self) -> Box<dyn DriverTearDownHandler> {
        Box::new(BuildkitDaemonTearDownHandler {})
    }
}

struct BuildkitDaemonTearDownHandler {}

impl DriverTearDownHandler for BuildkitDaemonTearDownHandler {
    fn tear_down(&self) -> Pin<Box<dyn futures_core::Future<Output = Result<(), GrpcError>>>> {
        Box::pin(futures_util::future::ok(()))
    }
}

impl super::Build for BuildkitDaemon {
    async fn docker_build(
        self,
        name: &str,
        frontend_opts: ImageBuildFrontendOptions,
        load_input: ImageBuildLoadInput,
        credentials: Option<HashMap<&str, DockerCredentials>>,
    ) -> Result<(), GrpcError> {
        let mut exporter_attrs = HashMap::new();
        exporter_attrs.insert(String::from("type"), String::from("docker"));
        exporter_attrs.insert(String::from("name"), String::from(name));
        super::solve(
            self,
            "moby",
            exporter_attrs,
            None,
            frontend_opts,
            load_input,
            credentials,
        )
        .await
    }
}

impl super::Export for BuildkitDaemon {
    async fn export(
        self,
        exporter_request: ImageExporterEnum,
        frontend_opts: ImageBuildFrontendOptions,
        load_input: ImageBuildLoadInput,
        credentials: Option<HashMap<&str, DockerCredentials>>,
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
        )
        .await
    }
}

impl super::Image for BuildkitDaemon {
    async fn registry(
        self,
        output: ImageRegistryOutput,
        frontend_opts: ImageBuildFrontendOptions,
        load_input: ImageBuildLoadInput,
        credentials: Option<HashMap<&str, DockerCredentials>>,
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
        )
        .await
    }
}
