#![cfg(feature = "buildkit")]

pub use bollard_buildkit_proto::fsutil;
pub use bollard_buildkit_proto::health;
pub use bollard_buildkit_proto::moby;

use std::collections::HashMap;
use std::path::Path;

use super::build::ImageBuildOutputCompression;

/// Parameters available for passing exporter output options to buildkit when exporting images
/// using a Solve GRPC request, f.e. used in associated [GRPC export methods](module@crate::grpc::export)
///
/// ## Examples
///
/// ```rust
/// use bollard::grpc::build::ImageBuildFrontendOptions;
///
/// let frontend_options = ImageBuildFrontendOptions::builder().pull(true).build();
///
/// ```
///
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageExporterOutput {
    pub(crate) name: String,
    pub(crate) compression: ImageBuildOutputCompression,
    pub(crate) compression_level: Option<u8>,
    pub(crate) force_compression: bool,
    pub(crate) oci_mediatypes: bool,
    pub(crate) annotation: HashMap<String, String>,
}

#[derive(Default, Debug, Copy, Clone, PartialEq)]
#[non_exhaustive]
/// Compression type for the exported image tar file
pub enum ImageExporterOCIOutputCompression {
    /// Emit the tar file uncompressed
    Uncompressed,
    /// Emit the tar file GZIP compressed
    #[default]
    Gzip,
    /// Emit the tar file as a stargz snapshot
    Estargz,
    /// Emit the tar file with lossless Zstandard compression
    Zstd,
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
/// use bollard::grpc::export::ImageExporterOutput;
/// use std::path::Path;
///
/// ImageExporterOutput::builder("docker.io/library/my-image:latest")
///     .dest(&Path::new("/tmp/oci.tar"));
///
/// ```
///
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageExporterRequest {
    pub(crate) output: ImageExporterOutput,
    pub(crate) path: std::path::PathBuf,
}

impl ImageExporterOutput {
    /// Constructs a [`ImageExporterOCIOutputBuilder`], the `name` parameter denotes the output
    /// image target, e.g. "docker.io/library/my-image:latest".
    pub fn builder(name: &str) -> ImageExporterOutputBuilder {
        ImageExporterOutputBuilder::new(name)
    }

    fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            ..Default::default()
        }
    }

    pub(crate) fn into_map(self) -> HashMap<String, String> {
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
/// use bollard::grpc::export::ImageExporterOutputBuilder;
/// use std::path::Path;
///
/// ImageExporterOutputBuilder::new("docker.io/library/my-image:latest")
///     .dest(&Path::new("/tmp/oci.tar"));
///
/// ```
///
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageExporterOutputBuilder {
    inner: ImageExporterOutput,
}

impl ImageExporterOutputBuilder {
    /// Constructs the builder given an image name, e.g. "docker.io/library/my-image:latest"
    pub fn new(name: &str) -> Self {
        Self {
            inner: ImageExporterOutput {
                name: String::from(name),
                ..Default::default()
            },
        }
    }

    /// Compression type, see [buildkit compression
    /// docs](https://docs.docker.com/build/exporters/#compression)
    pub fn compression(mut self, compression: &ImageBuildOutputCompression) -> Self {
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
    pub fn dest(self, path: &Path) -> ImageExporterRequest {
        ImageExporterRequest {
            output: self.inner,
            path: path.to_owned(),
        }
    }
}

// impl<'a> super::super::Docker {
//     ///
//     /// Export build result as [OCI image
//     /// layout](https://github.com/opencontainers/image-spec/blob/main/image-layout.md) tarball,
//     /// see [buildkit documentation on OCI
//     /// exporters](https://docs.docker.com/build/exporters/oci-docker/).
//     ///
//     /// <div class="warning">
//     ///  Warning: Buildkit features in Bollard are currently in Developer Preview and are intended strictly for feedback purposes only.
//     /// </div>
//     ///
//     /// # Arguments
//     ///
//     ///  - An owned instance of a [`DockerContainer`](crate::grpc::driver::docker_container::DockerContainer) is
//     ///  needed to create a grpc conncection with buildkit.
//     ///  - The `session_id` represents a unique id to identify the grpc connection.
//     ///  - An owned instance of a [`ImageBuildFrontendOptions`], to parameterise the buildkit
//     ///  frontend Solve request options.
//     ///  - An owned instance of a [`ImageExporterOCIRequest`], to parameterise the export specific
//     ///  buildkit options
//     ///  - An owned instance of a [`ImageExporterLoadInput`], to upload the Dockerfile that should
//     ///  be exported.
//     ///  - An optional hashmap of registry hosts to [credentials](crate::auth::DockerCredentials) to
//     ///  authenticate with, if using private images.
//     ///
//     /// ## Examples
//     ///
//     /// ```rust
//     /// # use bollard::grpc::driver::docker_container::DockerContainerBuilder;
//     /// # use bollard::grpc::export::ImageExporterLoadInput;
//     /// # use bollard::grpc::export::ImageExporterOCIOutputBuilder;
//     /// # use bollard::grpc::export::ImageBuildFrontendOptions;
//     /// # use bollard::Docker;
//     /// # use std::io::Write;
//     ///
//     /// # let mut docker = Docker::connect_with_socket_defaults().unwrap();
//     ///
//     /// let dockerfile = String::from(
//     ///     "FROM alpine as builder1
//     ///     RUN touch bollard.txt
//     ///     FROM alpine as builder2
//     ///     RUN --mount=type=bind,from=builder1,target=mnt cp mnt/bollard.txt buildkit-bollard.txt
//     ///     ENTRYPOINT ls buildkit-bollard.txt
//     ///     ",
//     /// );
//     ///
//     /// let mut header = tar::Header::new_gnu();
//     /// header.set_path("Dockerfile").unwrap();
//     /// # header.set_size(dockerfile.len() as u64);
//     /// # header.set_mode(0o755);
//     /// # header.set_cksum();
//     /// let mut tar = tar::Builder::new(Vec::new());
//     /// tar.append(&header, dockerfile.as_bytes()).unwrap();
//     ///
//     /// let uncompressed = tar.into_inner().unwrap();
//     /// let mut c = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
//     /// c.write_all(&uncompressed).unwrap();
//     /// let compressed = c.finish().unwrap();
//     ///
//     /// let session_id = "bollard-oci-export-buildkit-example";
//     ///
//     /// let frontend_opts = ImageBuildFrontendOptions::builder()
//     ///     .pull(true)
//     ///     .build();
//     ///
//     /// let output = ImageExporterOCIOutputBuilder::new(
//     ///     "docker.io/library/bollard-oci-export-buildkit-example:latest",
//     /// )
//     /// .annotation("exporter", "Bollard")
//     /// .dest(&std::path::Path::new("/tmp/oci-image.tar"));
//     ///
//     /// let buildkit_builder =
//     ///     DockerContainerBuilder::new("bollard_buildkit_export_oci_image", &docker, session_id);
//     ///
//     /// let load_input =
//     ///     ImageExporterLoadInput::Upload(bytes::Bytes::from(compressed));
//     ///
//     /// async move {
//     ///     let driver = buildkit_builder.bootstrap().await.unwrap();
//     ///     docker
//     ///         .image_export_oci(driver, session_id, frontend_opts, output, load_input, None)
//     ///         .await
//     ///         .unwrap();
//     /// };
//     /// ```
//     ///
//     pub async fn image_export_oci(
//         &mut self,
//         driver: DockerContainer,
//         session_id: &str,
//         frontend_opts: ImageBuildFrontendOptions,
//         exporter_request: ImageExporterRequest,
//         load_input: ImageBuildLoadInput,
//         credentials: Option<HashMap<&str, DockerCredentials>>,
//     ) -> Result<(), GrpcError> {
//         let buildkit_name = String::from(driver.name());

// let ImageExporterLoadInput::Upload(bytes) = load_input;

// let mut upload_provider = super::UploadProvider::new();
// let context = upload_provider.add(bytes.to_vec());

// let mut frontend_attrs = frontend_opts.into_map();
// frontend_attrs.insert(String::from("context"), context);
// let exporter_attrs = exporter_request.output.into_map();

//         let mut auth_provider = super::AuthProvider::new();
//         if let Some(creds) = credentials {
//             for (host, docker_credentials) in creds {
//                 auth_provider.set_docker_credentials(host, docker_credentials);
//             }
//         }
//         let auth = moby::filesync::v1::auth_server::AuthServer::new(auth_provider);

//         let filesend = moby::filesync::v1::file_send_server::FileSendServer::new(
//             super::FileSendImpl::new(exporter_request.path.as_path()),
//         );

//         let upload = moby::upload::v1::upload_server::UploadServer::new(upload_provider);

//         let services: Vec<super::GrpcServer> = vec![
//             super::GrpcServer::Auth(auth),
//             super::GrpcServer::FileSend(filesend),
//             super::GrpcServer::Upload(upload),
//         ];

//         let mut control_client =
//             crate::grpc::driver::Driver::grpc_handle(driver, session_id, services).await?;

//         let id = super::new_id();

// let solve_request = moby::buildkit::v1::SolveRequest {
//     r#ref: id,
//     cache: None,
//     definition: None,
//     entitlements: vec![],
//     exporter: String::from("oci"),
//     exporter_attrs,
//     frontend: String::from("dockerfile.v0"),
//     frontend_attrs,
//     frontend_inputs: HashMap::new(),
//     session: String::from(session_id),
// };

//         debug!("sending solve request: {:#?}", solve_request);
//         let res = control_client.solve(solve_request).await;
//         debug!("solve res: {:#?}", res);

//         // clean up
//         let kill = self
//             .kill_container(&buildkit_name, None::<KillContainerOptions<String>>)
//             .await;

//         trace!("kill res: {:#?}", kill);

//         res?;
//         kill?;

//         Ok(())
//     }
// }
