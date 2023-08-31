use std::{
    cmp,
    collections::HashMap,
    path::Path,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
//use std::io::Bytes;

use bollard_buildkit_proto::moby;
use bollard_stubs::models::{
    ContainerWaitExitError, ContainerWaitResponse, ExecInspectResponse, HostConfig, Mount,
    MountTypeEnum, SystemInfoCgroupDriverEnum, Volume,
};
use bytes::{BufMut, Bytes, BytesMut};
use futures_core::{ready, Future, TryStream};
use futures_util::{StreamExt, TryStreamExt};
use http::{
    header::{CONNECTION, UPGRADE},
    request::Builder,
    Method,
};
use hyper::Body;
use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::FramedRead;
use tonic::transport::server::Connected;
use tower_service::Service;

use crate::{
    container::{Config, CreateContainerOptions, LogOutput, WaitContainerOptions},
    errors::Error,
    exec::{CreateExecOptions, StartExecOptions, StartExecResults},
    image::CreateImageOptions,
    read::NewlineLogOutputDecoder,
    Docker,
};

const DEFAULT_IMAGE: &str = "moby/buildkit:master";
const DEFAULT_STATE_DIR: &str = "/var/lib/buildkit";
const DEFAULT_BUILDKIT_CONFIG_DIR: &str = "/etc/buildkit";

pin_project! {
    /// Reader for the [`into_async_read`](super::TryStreamExt::into_async_read) method.
    #[derive(Debug)]
    pub struct IntoAsyncRead<St>
    where
        St: TryStream<Error = std::io::Error>,
        St::Ok: AsRef<[u8]>,
    {
        #[pin]
        stream: St,
        state: ReadState<St::Ok>,
    }
}

#[derive(Debug)]
enum ReadState<T: AsRef<[u8]>> {
    Ready { chunk: T, chunk_start: usize },
    PendingChunk,
    Eof,
}

impl<St> IntoAsyncRead<St>
where
    St: TryStream<Error = std::io::Error>,
    St::Ok: AsRef<[u8]>,
{
    pub(super) fn new(stream: St) -> Self {
        Self {
            stream,
            state: ReadState::PendingChunk,
        }
    }
}

impl<St> AsyncRead for IntoAsyncRead<St>
where
    St: TryStream<Error = std::io::Error>,
    St::Ok: AsRef<[u8]>,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut this = self.project();

        loop {
            match this.state {
                ReadState::Ready { chunk, chunk_start } => {
                    let chunk = chunk.as_ref();
                    let len = cmp::min(buf.remaining(), chunk.len() - *chunk_start);

                    buf.put_slice(&chunk[*chunk_start..*chunk_start + len]);

                    *chunk_start += len;

                    if chunk.len() == *chunk_start {
                        *this.state = ReadState::PendingChunk;
                    }

                    return Poll::Ready(Ok(()));
                }
                ReadState::PendingChunk => match ready!(this.stream.as_mut().try_poll_next(cx)) {
                    Some(Ok(chunk)) => {
                        if !chunk.as_ref().is_empty() {
                            *this.state = ReadState::Ready {
                                chunk,
                                chunk_start: 0,
                            };
                        }
                    }
                    Some(Err(err)) => {
                        *this.state = ReadState::Eof;
                        return Poll::Ready(Err(err));
                    }
                    None => {
                        *this.state = ReadState::Eof;
                        return Poll::Ready(Ok(()));
                    }
                },
                ReadState::Eof => {
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

#[allow(missing_debug_implementations)]
/// TODO
pub struct GrpcFramedTransport {
    read: IntoAsyncRead<FramedRead<Pin<Box<dyn AsyncRead + Send>>, NewlineLogOutputDecoder>>,
    write: Pin<Box<dyn AsyncWrite + Send>>,
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

impl GrpcFramedTransport {
    pub(crate) fn new(
        read: Pin<Box<dyn AsyncRead + Send>>,
        write: Pin<Box<dyn AsyncWrite + Send>>,
        capacity: usize,
    ) -> Self {
        let output = FramedRead::with_capacity(read, NewlineLogOutputDecoder::new(true), capacity);
        let read = IntoAsyncRead::new(output);
        Self { read, write }
    }
}

impl Connected for GrpcFramedTransport {
    type ConnectInfo = ();

    fn connect_info(&self) -> Self::ConnectInfo {}
}

impl AsyncRead for GrpcFramedTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.read).poll_read(cx, buf)
    }
}

impl AsyncWrite for GrpcFramedTransport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.write).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.write).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.write).poll_shutdown(cx)
    }
}

impl Service<http::Uri> for Driver {
    type Response = GrpcFramedTransport;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: http::Uri) -> Self::Future {
        let client = Docker::clone(&self.docker);
        let name = String::clone(&self.name);
        let session_id = String::clone(&self.session_id);

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

        // Return the response as an immediate future
        Box::pin(fut)
    }
}

/// TODO
#[derive(Debug)]
pub struct DriverBuilder {
    inner: Driver,
}

impl DriverBuilder {
    /// TODO
    pub fn new(name: &str, docker: &Docker, session_id: &str) -> Self {
        Self {
            inner: Driver {
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
    pub async fn bootstrap(mut self) -> Result<Driver, Error> {
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
    pub fn network(&mut self, net: &str) -> &mut DriverBuilder {
        if net == "host" {
            self.inner
                .args
                .push(String::from("--allow-insecure-entitlement=network.host"));
        }

        self.inner.net_mode = Some(net.to_string());
        self
    }
}

/// TODO
#[derive(Debug)]
pub struct Driver {
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

impl Driver {
    /// TODO
    pub async fn create(&self) -> Result<(), Error> {
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
