#![cfg(feature = "buildkit")]

pub use bollard_buildkit_proto::fsutil;
pub use bollard_buildkit_proto::health;
pub use bollard_buildkit_proto::moby;

use bollard_buildkit_proto::moby::buildkit::v1::control_client::ControlClient;
use bytes::Bytes;
use futures_util::TryStreamExt;
use http::request::Builder;
use hyper::Body;
use hyper::Method;
use tonic::codegen::InterceptedService;
use tonic::service::Interceptor;
use tonic::transport::Channel;
use tonic::transport::Endpoint;

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::Path;

use crate::errors::Error;
use crate::grpc::driver::docker_container::DockerContainerBuilder;
use crate::grpc::GrpcTransport;

use super::io::into_async_read::IntoAsyncRead;

/// TODO
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageBuildFrontendOptions {
    //pub(crate) cgroupparent: Option<String>,
    //pub(crate) multiplatform: bool,
    //pub(crate) attests: HashMap<String, String>,
    pub(crate) cachefrom: Vec<String>,
    pub(crate) image_resolve_mode: bool,
    pub(crate) target: Option<String>,
    pub(crate) nocache: bool,
    pub(crate) buildargs: HashMap<String, String>,
    pub(crate) labels: HashMap<String, String>,
    pub(crate) platforms: Vec<ImageBuildPlatform>,
    pub(crate) force_network_mode: ImageBuildNetworkMode,
    pub(crate) extrahosts: Vec<ImageBuildHostIp>,
    pub(crate) shmsize: u64,
    //pub(crate) ulimit: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
/// TODO
pub struct ImageBuildHostIp {
    /// TODO
    pub host: String,
    /// TODO
    pub ip: IpAddr,
}

impl ToString for ImageBuildHostIp {
    fn to_string(&self) -> String {
        format!("{}={}", self.host, self.ip)
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[non_exhaustive]
/// TODO
pub enum ImageBuildNetworkMode {
    /// TODO
    Bridge,
    /// TODO
    Host,
    /// TODO
    None,
}

impl Default for ImageBuildNetworkMode {
    fn default() -> Self {
        ImageBuildNetworkMode::Bridge
    }
}

impl ToString for ImageBuildNetworkMode {
    fn to_string(&self) -> String {
        match self {
            ImageBuildNetworkMode::Bridge => String::from("default"),
            ImageBuildNetworkMode::Host => String::from("host"),
            ImageBuildNetworkMode::None => String::from("none"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
// from https://github.com/opencontainers/image-spec/blob/main/specs-go/v1/descriptor.go
/// TODO
pub struct ImageBuildPlatform {
    /// TODO
    pub architecture: String,
    /// TODO
    pub os: String,
    /// TODO
    pub variant: Option<String>,
}

impl ToString for ImageBuildPlatform {
    fn to_string(&self) -> String {
        let prefix = Path::new(&self.architecture).join(Path::new(&self.os));
        if let Some(variant) = &self.variant {
            prefix.join(Path::new(&variant))
        } else {
            prefix
        }
        .display()
        .to_string()
    }
}

impl ImageBuildFrontendOptions {
    /// TODO
    pub fn builder() -> ImageBuildFrontendOptionsBuilder {
        ImageBuildFrontendOptionsBuilder::new()
    }

    /// TODO
    pub fn to_map(self) -> HashMap<String, String> {
        let mut attrs = HashMap::new();

        if self.image_resolve_mode {
            attrs.insert(String::from("image-resolve-mode"), String::from("pull"));
        } else {
            attrs.insert(String::from("image-resolve-mode"), String::from("default"));
        }

        if let Some(target) = self.target {
            attrs.insert(String::from("target"), target);
        }

        if self.nocache {
            attrs.insert(String::from("no-cache"), String::new());
        }

        if !self.buildargs.is_empty() {
            for (key, value) in self.buildargs {
                attrs.insert(format!("build-arg:{}", key), value);
            }
        }

        if !self.labels.is_empty() {
            for (key, value) in self.labels {
                attrs.insert(format!("label:{}", key), value);
            }
        }

        if !self.platforms.is_empty() {
            attrs.insert(
                String::from("platform"),
                self.platforms
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }

        match self.force_network_mode {
            ImageBuildNetworkMode::Host => {
                attrs.insert(String::from("force-network-mode"), String::from("host"));
            }
            ImageBuildNetworkMode::None => {
                attrs.insert(String::from("force-network-mode"), String::from("none"));
            }
            _ => (),
        }

        if !self.extrahosts.is_empty() {
            attrs.insert(
                String::from("add-hosts"),
                self.extrahosts
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }

        if self.shmsize > 0 {
            attrs.insert(String::from("shm-size"), self.shmsize.to_string());
        }

        if !self.cachefrom.is_empty() {
            attrs.insert(String::from("cache-from"), self.cachefrom.join(","));
        }

        attrs
    }
}

/// TODO
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg(feature = "buildkit")]
pub struct ImageBuildFrontendOptionsBuilder {
    inner: ImageBuildFrontendOptions,
}

/// TODO
#[cfg(feature = "buildkit")]
impl ImageBuildFrontendOptionsBuilder {
    /// TODO
    pub fn new() -> Self {
        Self {
            inner: ImageBuildFrontendOptions {
                ..Default::default()
            },
        }
    }

    /// TODO
    pub fn cachefrom(mut self, value: &str) -> Self {
        self.inner.cachefrom.push(value.to_owned());
        self
    }

    /// TODO
    pub fn pull(mut self, pull: bool) -> Self {
        self.inner.image_resolve_mode = pull;
        self
    }

    /// TODO
    pub fn target(mut self, target: &str) -> Self {
        self.inner.target = Some(String::from(target));
        self
    }

    /// TODO
    pub fn nocache(mut self, nocache: bool) -> Self {
        self.inner.nocache = nocache;
        self
    }

    /// TODO
    pub fn buildarg(mut self, key: &str, value: &str) -> Self {
        self.inner
            .buildargs
            .insert(String::from(key), String::from(value));
        self
    }

    /// TODO
    pub fn label(mut self, key: &str, value: &str) -> Self {
        self.inner
            .labels
            .insert(String::from(key), String::from(value));
        self
    }

    /// TODO
    pub fn platforms(mut self, value: &ImageBuildPlatform) -> Self {
        self.inner.platforms.push(value.to_owned());
        self
    }

    /// TODO
    pub fn force_network_mode(mut self, value: &ImageBuildNetworkMode) -> Self {
        self.inner.force_network_mode = value.to_owned();
        self
    }

    /// TODO
    pub fn extrahost(mut self, value: &ImageBuildHostIp) -> Self {
        self.inner.extrahosts.push(value.to_owned());
        self
    }

    /// TODO
    pub fn shmsize(mut self, value: u64) -> Self {
        self.inner.shmsize = value;
        self
    }

    /// TODO
    pub fn build(self) -> ImageBuildFrontendOptions {
        self.inner
    }
}

/// TODO
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg(feature = "buildkit")]
pub struct ImageExporterOCIOutput {
    pub(crate) name: String,
    pub(crate) push: bool,
    pub(crate) push_by_digest: bool,
    pub(crate) insecure_registry: bool,
    pub(crate) dangling_name_prefix: Option<String>,
    pub(crate) name_canonical: Option<bool>,
    pub(crate) compression: ImageExporterOCIOutputCompression,
    pub(crate) compression_level: Option<u8>,
    pub(crate) force_compression: bool,
    pub(crate) oci_mediatypes: bool,
    pub(crate) unpack: bool,
    pub(crate) store: bool,
    pub(crate) annotation: HashMap<String, String>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[non_exhaustive]
/// TODO
pub enum ImageExporterOCIOutputCompression {
    /// TODO
    Uncompressed,
    /// TODO
    Gzip,
    /// TODO
    Estargz,
    /// TODO
    Zstd,
}

impl Default for ImageExporterOCIOutputCompression {
    fn default() -> Self {
        ImageExporterOCIOutputCompression::Gzip
    }
}

impl ToString for ImageExporterOCIOutputCompression {
    fn to_string(&self) -> String {
        match self {
            ImageExporterOCIOutputCompression::Uncompressed => "uncompressed",
            ImageExporterOCIOutputCompression::Gzip => "gzip",
            ImageExporterOCIOutputCompression::Estargz => "estargz",
            ImageExporterOCIOutputCompression::Zstd => "zstd",
        }
        .to_string()
    }
}

/// TODO
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg(feature = "buildkit")]
pub struct ImageExporterOCIRequest {
    output: ImageExporterOCIOutput,
    path: std::path::PathBuf,
}

/// TODO
#[cfg(feature = "buildkit")]
impl ImageExporterOCIOutput {
    /// TODO
    pub fn builder(&self, name: &str) -> ImageExporterOCIOutputBuilder {
        ImageExporterOCIOutputBuilder::new(name)
    }

    /// TODO
    pub fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            ..Default::default()
        }
    }

    /// TODO
    pub fn to_map(self) -> HashMap<String, String> {
        let mut attrs = HashMap::new();

        attrs.insert(String::from("name"), self.name);
        attrs.insert(String::from("push"), self.push.to_string());
        attrs.insert(
            String::from("push-by-digest"),
            self.push_by_digest.to_string(),
        );
        attrs.insert(
            String::from("registry.insecure"),
            self.insecure_registry.to_string(),
        );

        if let Some(dangling_name_prefix) = self.dangling_name_prefix {
            attrs.insert(String::from("dangling-name-prefix"), dangling_name_prefix);
        }

        if let Some(name_canonical) = self.name_canonical {
            attrs.insert(String::from("name-canonical"), name_canonical.to_string());
        }

        attrs.insert(String::from("compression"), self.compression.to_string());

        if let Some(compression_level) = self.compression_level {
            attrs.insert(
                String::from("compression-level"),
                compression_level.to_string(),
            );
        }

        attrs.insert(
            String::from("force-compression"),
            self.force_compression.to_string(),
        );
        attrs.insert(
            String::from("oci-mediatypes"),
            self.oci_mediatypes.to_string(),
        );
        attrs.insert(String::from("unpack"), self.unpack.to_string());
        attrs.insert(String::from("store"), self.store.to_string());

        for (key, value) in self.annotation {
            attrs.insert(format!("annotation.{}", key), value);
        }

        attrs
    }
}

/// TODO
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg(feature = "buildkit")]
pub struct ImageExporterOCIOutputBuilder {
    inner: ImageExporterOCIOutput,
}

/// TODO
#[cfg(feature = "buildkit")]
impl ImageExporterOCIOutputBuilder {
    /// TODO
    pub fn new(name: &str) -> Self {
        Self {
            inner: ImageExporterOCIOutput {
                name: String::from(name),
                ..Default::default()
            },
        }
    }

    /// TODO
    pub fn push(mut self, push: bool) -> Self {
        self.inner.push = push;
        self
    }

    /// TODO
    pub fn push_by_digest(mut self, push_by_digest: bool) -> Self {
        self.inner.push_by_digest = push_by_digest;
        self
    }

    /// TODO
    pub fn insecure_registry(mut self, insecure_registry: bool) -> Self {
        self.inner.insecure_registry = insecure_registry;
        self
    }

    /// TODO
    pub fn dangling_name_prefix(mut self, dangling_name_prefix: &str) -> Self {
        self.inner.dangling_name_prefix = Some(String::from(dangling_name_prefix));
        self
    }

    /// TODO
    pub fn name_canonical(mut self, name_canonical: bool) -> Self {
        self.inner.name_canonical = Some(name_canonical);
        self
    }

    /// TODO
    pub fn compression(mut self, compression: &ImageExporterOCIOutputCompression) -> Self {
        self.inner.compression = compression.to_owned();
        self
    }

    /// TODO
    pub fn compression_level(mut self, compression_level: u8) -> Self {
        self.inner.compression_level = Some(compression_level);
        self
    }

    /// TODO
    pub fn force_compression(mut self, force_compression: bool) -> Self {
        self.inner.force_compression = force_compression;
        self
    }

    /// TODO
    pub fn oci_mediatypes(mut self, oci_mediatypes: bool) -> Self {
        self.inner.oci_mediatypes = oci_mediatypes;
        self
    }

    /// TODO
    pub fn unpack(mut self, unpack: bool) -> Self {
        self.inner.unpack = unpack;
        self
    }

    /// TODO
    pub fn store(mut self, store: bool) -> Self {
        self.inner.store = store;
        self
    }

    /// TODO
    pub fn annotation(mut self, key: &str, value: &str) -> Self {
        self.inner
            .annotation
            .insert(String::from(key), String::from(value));
        self
    }

    /// TODO
    pub fn dest(self, path: &Path) -> ImageExporterOCIRequest {
        ImageExporterOCIRequest {
            output: self.inner,
            path: path.to_owned(),
        }
    }
}

#[derive(Debug, PartialEq)]
#[non_exhaustive]
/// TODO
pub enum ImageExporterLoadInput {
    /// TODO
    Upload(Bytes),
}

impl super::super::Docker {
    /// TODO
    async fn raw_grpc_handle(
        &mut self,
        session_id: &str,
        services: Vec<super::GrpcServer>,
    ) -> Result<Channel, Error> {
        let grpc_client = super::GrpcClient {
            client: self.clone(),
            session_id: String::from(session_id),
        };

        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(grpc_client)
            .await?;

        let url = "/session";

        let opt: Option<serde_json::Value> = None;
        let req = self.build_request(
            &url,
            Builder::new()
                .method(Method::POST)
                .header("Connection", "Upgrade")
                .header("Upgrade", "h2c")
                .header("X-Docker-Expose-Session-Uuid", session_id)
                .header(
                    "X-Docker-Expose-Session-Grpc-Method",
                    "/moby.filesync.v1.FileSend/diffcopy",
                ),
            opt,
            Ok(Body::empty()),
        );

        let (read, write) = self.process_upgraded(req).await?;

        let output = Box::pin(read);
        let input = Box::pin(write);
        let transport = GrpcTransport {
            read: output,
            write: input,
        };

        tokio::spawn(async {
            let health = health::health_server::HealthServer::new(super::HealthServerImpl::new());
            let mut builder = tonic::transport::Server::builder();
            let mut router = builder.add_service(health);
            for service in services {
                router = service.append(router);
            }
            debug!("router: {:#?}", router);
            router
                .serve_with_incoming(futures_util::stream::iter(vec![Ok::<
                    _,
                    tonic::transport::Error,
                >(
                    transport
                )]))
                .await;
        });

        Ok(channel)
    }

    /// TODO
    async fn container_grpc_handle(
        &mut self,
        session_id: &str,
        services: Vec<super::GrpcServer>,
    ) -> Result<ControlClient<InterceptedService<Channel, impl Interceptor>>, Error> {
        let builder = DockerContainerBuilder::new("bollard_buildkit", self, session_id);

        let driver = builder.bootstrap().await?;

        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(driver)
            .await?;

        let mut control_client =
            moby::buildkit::v1::control_client::ControlClient::with_interceptor(
                channel,
                move |mut req: tonic::Request<()>| {
                    let metadata = req.metadata_mut();

                    metadata.insert(
                        "x-docker-expose-session-uuid",
                        "bollard-oci-export-buildkit-example".parse().unwrap(),
                    );
                    metadata.insert(
                        "x-docker-expose-session-grpc-method",
                        "/moby.filesync.v1.FileSend/diffcopy".parse().unwrap(),
                    );

                    Ok(req)
                },
            );

        let (asyncwriter, asyncreader) = tokio::io::duplex(8 * 1024);
        let streamreader = super::io::reader_stream::ReaderStream::new(asyncreader);
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
            let health = health::health_server::HealthServer::new(super::HealthServerImpl::new());
            let mut builder = tonic::transport::Server::builder();
            let mut router = builder.add_service(health);
            for service in services {
                router = service.append(router);
            }
            trace!("router: {:#?}", router);
            router
                .serve_with_incoming(futures_util::stream::iter(vec![Ok::<
                    _,
                    tonic::transport::Error,
                >(
                    transport
                )]))
                .await;
        });

        Ok(control_client)
    }

    /// TODO
    #[cfg(feature = "buildkit")]
    pub async fn image_export_oci(
        &mut self,
        session_id: &str,
        frontend_opts: ImageBuildFrontendOptions,
        exporter_request: ImageExporterOCIRequest,
        load_input: ImageExporterLoadInput,
    ) -> Result<(), Error> {
        let payload = match load_input {
            ImageExporterLoadInput::Upload(bytes) => bytes,
        };

        let mut upload_provider = super::UploadProvider::new();
        let context = upload_provider.add(payload.to_vec());

        let mut frontend_attrs = frontend_opts.to_map();
        frontend_attrs.insert(String::from("context"), context);
        let exporter_attrs = exporter_request.output.to_map();

        let filesend = moby::filesync::v1::file_send_server::FileSendServer::new(
            super::FileSendImpl::new(exporter_request.path.as_path()),
        );

        let upload = moby::upload::v1::upload_server::UploadServer::new(upload_provider);

        let services: Vec<super::GrpcServer> = vec![
            super::GrpcServer::FileSend(filesend),
            super::GrpcServer::Upload(upload),
        ];

        let mut control_client = self
            .container_grpc_handle(session_id, services)
            .await
            .unwrap();

        let id = super::new_id();

        let solve_request = moby::buildkit::v1::SolveRequest {
            r#ref: String::from(id),
            cache: None,
            definition: None,
            entitlements: vec![],
            exporter: String::from("oci"),
            exporter_attrs,
            frontend: String::from("dockerfile.v0"),
            frontend_attrs,
            frontend_inputs: HashMap::new(),
            session: String::from(session_id),
        };

        trace!("sending solve request: {:#?}", solve_request);
        let res = control_client.solve(solve_request).await;
        trace!("solve res: {:#?}", res);

        Ok(())
    }
}
