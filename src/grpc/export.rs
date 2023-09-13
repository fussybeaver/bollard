#![cfg(feature = "buildkit")]

pub use bollard_buildkit_proto::fsutil;
pub use bollard_buildkit_proto::health;
pub use bollard_buildkit_proto::moby;

use bytes::Bytes;

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::Path;

use crate::container::KillContainerOptions;
use crate::errors::Error;

use super::driver::docker_container::DockerContainer;

/// Parameters available for passing frontend options to buildkit when initiating a Solve GRPC
/// request, f.e. used in associated methods within the [GRPC module](module@crate::grpc)
///
/// ## Examples
///
/// ```rust
/// use bollard::grpc::export::ImageBuildFrontendOptions;
///
/// ImageBuildFrontendOptions::builder().pull(true).build();
///
/// ```
///
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
/// A list of hostnames/IP mappings to add to the container's `/etc/hosts` file.
pub struct ImageBuildHostIp {
    /// The hosname mapping component of a hostname/IP mapping
    pub host: String,
    /// The IP mapping component of a hostname/IP mapping
    pub ip: IpAddr,
}

impl ToString for ImageBuildHostIp {
    fn to_string(&self) -> String {
        format!("{}={}", self.host, self.ip)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
/// Network mode to use for this container. Supported standard values are: `bridge`, `host`,
/// `none`, and `container:<name|id>`. Any other value is taken as a custom network's name to which
/// this container should connect to.
pub enum ImageBuildNetworkMode {
    /// Bridge mode networking
    Bridge,
    /// Host mode networking
    Host,
    /// No networking mode
    None,
    /// Container mode networking, with container name as `name`
    Container(String),
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
            ImageBuildNetworkMode::Container(name) => format!("container:{name}"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
/// Describes the platform which the image in the manifest runs on, as defined in the [OCI Image Index Specification](https://github.com/opencontainers/image-spec/blob/v1.0.1/image-index.md).
pub struct ImageBuildPlatform {
    /// The CPU architecture, for example `amd64` or `ppc64`.
    pub architecture: String,
    /// The operating system, for example `linux` or `windows`.
    pub os: String,
    /// Optional field specifying a variant of the CPU, for example `v7` to specify ARMv7 when architecture is `arm`.
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
    /// Construct a builder for the `ImageBuildFrontendOptions`
    pub fn builder() -> ImageBuildFrontendOptionsBuilder {
        ImageBuildFrontendOptionsBuilder::new()
    }

    pub(crate) fn to_map(self) -> HashMap<String, String> {
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

/// Builder for the associated [`ImageBuildFrontendOptions`](ImageBuildFrontendOptions) type
///
/// ## Examples
///
/// ```rust
/// use bollard::grpc::export::ImageBuildFrontendOptionsBuilder;
///
/// ImageBuildFrontendOptionsBuilder::new().pull(true).build();
///
/// ```
///
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageBuildFrontendOptionsBuilder {
    inner: ImageBuildFrontendOptions,
}

impl ImageBuildFrontendOptionsBuilder {
    /// Construct a new builder
    pub fn new() -> Self {
        Self {
            inner: ImageBuildFrontendOptions {
                ..Default::default()
            },
        }
    }

    /// Image to add towards for build cache resolution.
    pub fn cachefrom(mut self, value: &str) -> Self {
        self.inner.cachefrom.push(value.to_owned());
        self
    }

    /// Attempt to pull the image even if an older image exists locally.
    pub fn pull(mut self, pull: bool) -> Self {
        self.inner.image_resolve_mode = pull;
        self
    }

    /// A name and optional tag to apply to the image in the `name:tag` format. If you omit the tag
    /// the default `latest` value is assumed. You can provide several `t` parameters.
    pub fn target(mut self, target: &str) -> Self {
        self.inner.target = Some(String::from(target));
        self
    }

    /// Do not use the cache when building the image.
    pub fn nocache(mut self, nocache: bool) -> Self {
        self.inner.nocache = nocache;
        self
    }

    /// Add string pair for build-time variables. Users pass these values at build-time.
    /// Docker uses the buildargs as the environment context for commands run via the `Dockerfile`
    /// RUN instruction, or for variable expansion in other `Dockerfile` instructions.
    pub fn buildarg(mut self, key: &str, value: &str) -> Self {
        self.inner
            .buildargs
            .insert(String::from(key), String::from(value));
        self
    }

    /// Append arbitrary key/value label to set on the image.
    pub fn label(mut self, key: &str, value: &str) -> Self {
        self.inner
            .labels
            .insert(String::from(key), String::from(value));
        self
    }

    /// Platform in the format [`ImageBuildPlatform`](ImageBuildPlatform)
    pub fn platforms(mut self, value: &ImageBuildPlatform) -> Self {
        self.inner.platforms.push(value.to_owned());
        self
    }

    /// Sets the networking mode for the run commands during build. Supported standard values are:
    /// `bridge`, `host`, `none`, and `container:<name|id>`.
    pub fn force_network_mode(mut self, value: &ImageBuildNetworkMode) -> Self {
        self.inner.force_network_mode = value.to_owned();
        self
    }

    /// Extra hosts to add to `/etc/hosts`.
    pub fn extrahost(mut self, value: &ImageBuildHostIp) -> Self {
        self.inner.extrahosts.push(value.to_owned());
        self
    }

    /// Size of `/dev/shm` in bytes. The size must be greater than 0. If omitted the system uses 64MB.
    pub fn shmsize(mut self, value: u64) -> Self {
        self.inner.shmsize = value;
        self
    }

    /// Consume the builder and emit an [`ImageBuildFrontendOptions`](ImageBuildFrontendOptions)
    pub fn build(self) -> ImageBuildFrontendOptions {
        self.inner
    }
}

/// Parameters available for passing exporter output options to buildkit when exporting images
/// using a Solve GRPC request, f.e. used in associated [GRPC export methods](module@crate::grpc::export)
///
/// ## Examples
///
/// ```rust
/// use bollard::grpc::export::ImageBuildFrontendOptions;
///
/// let frontend_options = ImageBuildFrontendOptions::builder().pull(true).build();
///
/// ```
///
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageExporterOCIOutput {
    pub(crate) name: String,
    pub(crate) compression: ImageExporterOCIOutputCompression,
    pub(crate) compression_level: Option<u8>,
    pub(crate) force_compression: bool,
    pub(crate) oci_mediatypes: bool,
    pub(crate) annotation: HashMap<String, String>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[non_exhaustive]
/// Compression type for the exported image tar file
pub enum ImageExporterOCIOutputCompression {
    /// Emit the tar file uncompressed
    Uncompressed,
    /// Emit the tar file GZIP compressed
    Gzip,
    /// Emit the tar file as a stargz snapshot
    Estargz,
    /// Emit the tar file with lossless Zstandard compression
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

/// Request struct to parameterise export OCI images as part of the
/// [`image_export_oci`][crate::Docker::image_export_oci] Docker/buildkit functionality.
///
/// Constructed through the [`ImageExporterOCIOutputBuilder`] type.
///
/// ## Examples
///
/// ```rust
/// use bollard::grpc::export::ImageExporterOCIOutput;
/// use std::path::Path;
///
/// ImageExporterOCIOutput::builder("docker.io/library/my-image:latest")
///     .dest(&Path::new("/tmp/oci.tar"));
///
/// ```
///
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg(feature = "buildkit")]
pub struct ImageExporterOCIRequest {
    output: ImageExporterOCIOutput,
    path: std::path::PathBuf,
}

#[cfg(feature = "buildkit")]
impl ImageExporterOCIOutput {
    /// Constructs a [`ImageExporterOCIOutputBuilder`], the `name` parameter denotes the output
    /// image target, e.g. "docker.io/library/my-image:latest".
    pub fn builder(name: &str) -> ImageExporterOCIOutputBuilder {
        ImageExporterOCIOutputBuilder::new(name)
    }

    fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            ..Default::default()
        }
    }

    pub(crate) fn to_map(self) -> HashMap<String, String> {
        let mut attrs = HashMap::new();

        attrs.insert(String::from("name"), self.name);
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

        for (key, value) in self.annotation {
            attrs.insert(format!("annotation.{}", key), value);
        }

        attrs
    }
}

/// Builder used to parameterise export OCI images as part of the
/// [`image_export_oci`][crate::Docker::image_export_oci] Docker/buildkit functionality.
///
/// ## Examples
///
/// ```rust
/// use bollard::grpc::export::ImageExporterOCIOutputBuilder;
/// use std::path::Path;
///
/// ImageExporterOCIOutputBuilder::new("docker.io/library/my-image:latest")
///     .dest(&Path::new("/tmp/oci.tar"));
///
/// ```
///
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageExporterOCIOutputBuilder {
    inner: ImageExporterOCIOutput,
}

impl ImageExporterOCIOutputBuilder {
    /// Constructs the builder given an image name, e.g. "docker.io/library/my-image:latest"
    pub fn new(name: &str) -> Self {
        Self {
            inner: ImageExporterOCIOutput {
                name: String::from(name),
                ..Default::default()
            },
        }
    }

    /// Compression type, see [buildkit compression
    /// docs](https://docs.docker.com/build/exporters/#compression)
    pub fn compression(mut self, compression: &ImageExporterOCIOutputCompression) -> Self {
        self.inner.compression = compression.to_owned();
        self
    }

    /// Compression level, see [buildkit compression
    /// docs](https://docs.docker.com/build/exporters/#compression)
    pub fn compression_level(mut self, compression_level: u8) -> Self {
        self.inner.compression_level = Some(compression_level);
        self
    }

    /// Forcefully apply compression, see [buildkit compression
    /// docs](https://docs.docker.com/build/exporters/#compression)
    pub fn force_compression(mut self, force_compression: bool) -> Self {
        self.inner.force_compression = force_compression;
        self
    }

    /// Use OCI media types in exporter manifests. Defaults to `true` for `type=oci`, and `false`
    /// for `type=docker`. See [buildkit OCI media types
    /// docs](https://docs.docker.com/build/exporters/#oci-media-types)
    pub fn oci_mediatypes(mut self, oci_mediatypes: bool) -> Self {
        self.inner.oci_mediatypes = oci_mediatypes;
        self
    }

    /// Attach an annotation with the respective `key` and `value` to the built image, see
    /// [buildkit annotations
    /// docs](https://docs.docker.com/build/exporters/oci-docker/#annotations)
    pub fn annotation(mut self, key: &str, value: &str) -> Self {
        self.inner
            .annotation
            .insert(String::from(key), String::from(value));
        self
    }

    /// Consume this builder to create an [`ImageExporterOCIOutput`] for the
    /// [`image_export_oci`](crate::Docker::image_export_oci) method
    pub fn dest(self, path: &Path) -> ImageExporterOCIRequest {
        ImageExporterOCIRequest {
            output: self.inner,
            path: path.to_owned(),
        }
    }
}

#[derive(Debug, PartialEq)]
#[non_exhaustive]
/// Dockerfile seed implementation to export OCI images as part of the
/// [`image_export_oci`][crate::Docker::image_export_oci] Docker/buildkit functionality.
///
/// Accepts a compressed Dockerfile as Bytes
///
/// ## Examples
///
/// ```rust
///     # use std::io::Write;
///
///     let dockerfile = String::from(
///         "FROM alpine as builder1
///         RUN touch bollard.txt
///         FROM alpine as builder2
///         RUN --mount=type=bind,from=builder1,target=mnt cp mnt/bollard.txt buildkit-bollard.txt
///         ENTRYPOINT ls buildkit-bollard.txt
///         ",
///     );
///     let mut header = tar::Header::new_gnu();
///     header.set_path("Dockerfile").unwrap();
///     # header.set_size(dockerfile.len() as u64);
///     # header.set_mode(0o755);
///     # header.set_cksum();
///     let mut tar = tar::Builder::new(Vec::new());
///     tar.append(&header, dockerfile.as_bytes()).unwrap();
///     let uncompressed = tar.into_inner().unwrap();
///     let mut c = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
///     c.write_all(&uncompressed).unwrap();
///     let compressed = c.finish().unwrap();
///
///     bollard::grpc::export::ImageExporterLoadInput::Upload(bytes::Bytes::from(compressed));
///
/// ```
///
pub enum ImageExporterLoadInput {
    /// Seed the exporter with a tarball containing the Dockerfile to build
    Upload(Bytes),
}

impl<'a> super::super::Docker {
    ///
    /// Export build result as [OCI image
    /// layout](https://github.com/opencontainers/image-spec/blob/main/image-layout.md) tarball,
    /// see [buildkit documentation on OCI
    /// exporters](https://docs.docker.com/build/exporters/oci-docker/).
    ///
    /// <div class="warning">
    ///  Warning: Buildkit features in Bollard are currently in Developer Preview and are intended strictly for feedback purposes only.
    /// </div>
    ///
    /// # Arguments
    ///
    ///  - An owned instance of a [`DockerContainer`](crate::grpc::driver::docker_container::DockerContainer) is
    ///  needed to create a grpc conncection with buildkit.
    ///  - The `session_id` represents a unique id to identify the grpc connection.
    ///  - An owned instance of a [`ImageBuildFrontendOptions`], to parameterise the buildkit
    ///  frontend Solve request options.
    ///  - An owned instance of a [`ImageExporterOCIRequest`], to parameterise the export specific
    ///  buildkit options
    ///  - An owned instance of a [`ImageExporterLoadInput`], to upload the Dockerfile that should
    ///  be exported.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// # use bollard::grpc::driver::docker_container::DockerContainerBuilder;
    /// # use bollard::grpc::export::ImageExporterLoadInput;
    /// # use bollard::grpc::export::ImageExporterOCIOutputBuilder;
    /// # use bollard::grpc::export::ImageBuildFrontendOptions;
    /// # use bollard::Docker;
    /// # use std::io::Write;
    ///
    /// # let mut docker = Docker::connect_with_socket_defaults().unwrap();
    ///
    /// let dockerfile = String::from(
    ///     "FROM alpine as builder1
    ///     RUN touch bollard.txt
    ///     FROM alpine as builder2
    ///     RUN --mount=type=bind,from=builder1,target=mnt cp mnt/bollard.txt buildkit-bollard.txt
    ///     ENTRYPOINT ls buildkit-bollard.txt
    ///     ",
    /// );
    ///
    /// let mut header = tar::Header::new_gnu();
    /// header.set_path("Dockerfile").unwrap();
    /// # header.set_size(dockerfile.len() as u64);
    /// # header.set_mode(0o755);
    /// # header.set_cksum();
    /// let mut tar = tar::Builder::new(Vec::new());
    /// tar.append(&header, dockerfile.as_bytes()).unwrap();
    ///
    /// let uncompressed = tar.into_inner().unwrap();
    /// let mut c = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    /// c.write_all(&uncompressed).unwrap();
    /// let compressed = c.finish().unwrap();
    ///
    /// let session_id = "bollard-oci-export-buildkit-example";
    ///
    /// let frontend_opts = ImageBuildFrontendOptions::builder()
    ///     .pull(true)
    ///     .build();
    ///
    /// let output = ImageExporterOCIOutputBuilder::new(
    ///     "docker.io/library/bollard-oci-export-buildkit-example:latest",
    /// )
    /// .annotation("exporter", "Bollard")
    /// .dest(&std::path::Path::new("/tmp/oci-image.tar"));
    ///
    /// let buildkit_builder =
    ///     DockerContainerBuilder::new("bollard_buildkit_export_oci_image", &docker, session_id);
    ///
    /// let load_input =
    ///     ImageExporterLoadInput::Upload(bytes::Bytes::from(compressed));
    ///
    /// async move {
    ///     let driver = buildkit_builder.bootstrap().await.unwrap();
    ///     docker
    ///         .image_export_oci(driver, session_id, frontend_opts, output, load_input)
    ///         .await
    ///         .unwrap();
    /// };
    /// ```
    ///
    pub async fn image_export_oci(
        &mut self,
        driver: DockerContainer,
        session_id: &str,
        frontend_opts: ImageBuildFrontendOptions,
        exporter_request: ImageExporterOCIRequest,
        load_input: ImageExporterLoadInput,
    ) -> Result<(), Error> {
        let buildkit_name = String::from(driver.name());

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

        let mut control_client = driver.grpc_handle(session_id, services).await.unwrap();

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

        debug!("sending solve request: {:#?}", solve_request);
        let res = control_client.solve(solve_request).await;
        debug!("solve res: {:#?}", res);

        // clean up
        let kill = self
            .kill_container(&buildkit_name, None::<KillContainerOptions<String>>)
            .await;

        trace!("kill res: {:#?}", kill);

        res?;
        kill?;

        Ok(())
    }
}
