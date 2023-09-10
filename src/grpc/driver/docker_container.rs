#![cfg(feature = "buildkit")]

use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use bollard_buildkit_proto::{
    health::health_server::HealthServer, moby::buildkit::v1::control_client::ControlClient,
};
use bollard_stubs::models::{
    ExecInspectResponse, HostConfig, Mount, MountTypeEnum, SystemInfoCgroupDriverEnum,
};
use bytes::BytesMut;
use futures_core::Future;
use futures_util::{StreamExt, TryStreamExt};
use http::{
    header::{CONNECTION, UPGRADE},
    request::Builder,
    Method,
};
use tonic::{codegen::InterceptedService, transport::Channel};
use tonic::{service::Interceptor, transport::Endpoint};
use tower_service::Service;

use crate::{
    container::{Config, CreateContainerOptions},
    errors::Error,
    exec::{CreateExecOptions, StartExecOptions, StartExecResults},
    grpc::{
        io::{
            into_async_read::IntoAsyncRead, reader_stream::ReaderStream, GrpcFramedTransport,
            GrpcTransport,
        },
        GrpcServer, HealthServerImpl,
    },
    image::CreateImageOptions,
    Docker,
};

const DEFAULT_IMAGE: &str = "moby/buildkit:master";
const DEFAULT_STATE_DIR: &str = "/var/lib/buildkit";
const DUPLEX_BUF_SIZE: usize = 8 * 1024;

impl Service<http::Uri> for DockerContainer {
    type Response = GrpcFramedTransport;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: http::Uri) -> Self::Future {
        let client = Docker::clone(&self.docker);
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

            client
                .process_upgraded(req)
                .await
                .and_then(|(read, write)| {
                    let output = Box::pin(read);
                    let input = Box::pin(write);
                    Ok(GrpcFramedTransport::new(output, input, capacity))
                })
        };

        Box::pin(fut)
    }
}

/// TODO
#[derive(Debug)]
pub struct DockerContainerBuilder {
    inner: DockerContainer,
}

impl DockerContainerBuilder {
    /// TODO
    pub fn new(name: &str, docker: &Docker, session_id: &str) -> Self {
        Self {
            inner: DockerContainer {
                name: String::from(name),
                docker: Docker::clone(docker),
                session_id: String::from(session_id),
                net_mode: None,
                image: None,
                cgroup_parent: None,
                env: vec![],
                args: vec![],
            },
        }
    }

    /// TODO
    pub async fn bootstrap(mut self) -> Result<DockerContainer, Error> {
        debug!("booting buildkit");

        if self.inner.net_mode.is_none() {
            self.network("host");
        }

        let container_name = &self.inner.name;
        match self
            .inner
            .docker
            .inspect_container(&self.inner.name, None)
            .await
        {
            Err(Error::DockerResponseServerError {
                status_code: 404,
                message: _,
            }) => self.inner.create().await?,
            _ => (),
        };

        debug!("starting container {}", &container_name);

        self.inner.start().await?;
        self.inner.wait().await?;

        Ok(self.inner)
    }

    /// TODO
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
}

/// TODO
#[derive(Debug)]
pub struct DockerContainer {
    /// TODO
    name: String,
    /// TODO
    docker: Docker,
    /// TODO
    session_id: String,
    /// TODO
    net_mode: Option<String>,
    /// TODO
    image: Option<String>,
    /// TODO
    cgroup_parent: Option<String>,
    /// TODO
    env: Vec<String>,
    /// TODO
    args: Vec<String>,
}

impl<'a> DockerContainer {
    /// TODO
    pub fn name(&self) -> &str {
        &self.name
    }

    /// TODO
    pub(crate) async fn grpc_handle(
        self,
        session_id: &'a str,
        services: Vec<GrpcServer>,
    ) -> Result<ControlClient<InterceptedService<Channel, impl Interceptor + 'a>>, Error> {
        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(self)
            .await?;

        let metadata_grpc_method: Vec<String> = services.iter().flat_map(|s| s.names()).collect();

        let mut control_client =
            ControlClient::with_interceptor(channel, move |mut req: tonic::Request<()>| {
                let metadata = req.metadata_mut();

                metadata.insert(
                    "x-docker-expose-session-uuid",
                    session_id.parse().map_err(|_| {
                        tonic::Status::invalid_argument("invalid 'session_id' argument")
                    })?,
                );

                debug!("grpc-method: {:?}", &metadata_grpc_method.join(","));
                for metadata_grpc_method_value in &metadata_grpc_method {
                    metadata.append(
                        "x-docker-expose-session-grpc-method",
                        metadata_grpc_method_value.parse().map_err(|_| {
                            tonic::Status::invalid_argument("invalid grpc method name")
                        })?,
                    );
                }

                Ok(req)
            });

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
            match router
                .serve_with_incoming(futures_util::stream::iter(vec![Ok::<
                    _,
                    tonic::transport::Error,
                >(
                    transport
                )]))
                .await
            {
                Err(e) => error!("Failed to serve grpc connection: {}", e),
                _ => (),
            }
        });

        Ok(control_client)
    }

    async fn create(&self) -> Result<(), Error> {
        let image_name = if let Some(image) = &self.image {
            image
        } else {
            DEFAULT_IMAGE
        };

        debug!("pulling image {}", &image_name);

        // TODO: registry auth

        let create_image_options = CreateImageOptions {
            from_image: String::from(image_name),
            ..Default::default()
        };

        self.docker
            .create_image(Some(create_image_options), None, None)
            .try_collect::<Vec<_>>()
            .await?;

        debug!("creating container {}", &self.name);

        let container_options = CreateContainerOptions {
            name: String::from(&self.name),
            ..Default::default()
        };

        let info = self.docker.info().await?;
        let cgroup_parent = match &info.cgroup_driver {
            Some(SystemInfoCgroupDriverEnum::CGROUPFS) =>
            // place all buildkit containers into this cgroup
            {
                Some(if let Some(cgroup_parent) = &self.cgroup_parent {
                    String::clone(&cgroup_parent)
                } else {
                    String::from("/docker/buildx")
                })
            }
            _ => None,
        };

        let network_mode = if let Some(net_mode) = &self.net_mode {
            Some(String::clone(&net_mode))
        } else {
            None
        };

        let userns_mode = if let Some(security_options) = &info.security_options {
            if security_options.iter().any(|f| f == "userns") {
                Some(String::from("host"))
            } else {
                None
            }
        } else {
            None
        };

        let host_config = HostConfig {
            privileged: Some(true),
            mounts: Some(vec![Mount {
                typ: Some(MountTypeEnum::VOLUME),
                source: Some(format!("{}_state", &self.name)),
                target: Some(String::from(DEFAULT_STATE_DIR)),
                ..Default::default()
            }]),
            init: Some(true),
            network_mode,
            cgroup_parent,
            userns_mode,
            ..Default::default()
        };

        let container_config = Config {
            image: Some(String::from(image_name)),
            env: Some(Vec::clone(&self.env)),
            host_config: Some(host_config),
            cmd: Some(Vec::clone(&self.args)),
            ..Default::default()
        };

        self.docker
            .create_container(Some(container_options), container_config)
            .await?;

        self.start().await?;

        self.wait().await?;

        Ok(())
    }

    async fn start(&self) -> Result<(), Error> {
        self.docker
            .start_container::<String>(&self.name, None)
            .await?;

        Ok(())
    }

    async fn wait(&self) -> Result<(), Error> {
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
                    return Err(Error::DockerContainerWaitError {
                        error: String::from(std::str::from_utf8(stdout.as_ref())?),
                        code: status_code,
                    });
                }
                _ => {
                    tokio::time::sleep(Duration::from_millis(attempts * 120)).await;
                    attempts = attempts + 1;
                }
            }
        }
    }
}
