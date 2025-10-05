//! Image API: creating, manipulating and pushing docker images
#![allow(deprecated)]
#[cfg(feature = "buildkit")]
use bollard_buildkit_proto::moby::filesync::packet::file_send_server::FileSendServer as FileSendPacketServer;
use bytes::Bytes;
use futures_core::Stream;
#[cfg(feature = "buildkit")]
use futures_util::future::{Either, FutureExt};
#[cfg(feature = "buildkit")]
use futures_util::stream;
use futures_util::stream::StreamExt;
use http::header::CONTENT_TYPE;
use http::request::Builder;
use http_body_util::Full;
use hyper::Method;

use super::Docker;
use crate::auth::{DockerCredentials, DockerCredentialsHeader};
use crate::docker::{body_stream, BodyType};
use crate::errors::Error;
use crate::models::*;

use std::collections::HashMap;

enum ImageBuildBuildkitEither {
    #[allow(dead_code)]
    Left(Option<HashMap<String, DockerCredentials>>),
    Right(Option<HashMap<String, DockerCredentials>>),
}

impl Docker {
    /// ---
    ///
    /// # List Images
    ///
    /// Returns a list of images on the server. Note that it uses a different, smaller
    /// representation of an image than inspecting a single image
    ///
    /// # Arguments
    ///
    ///  - An optional [List Images Options](ListImagesOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [API Images](ImageSummary), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::ListImagesOptions;
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("dangling", vec!["true"]);
    ///
    /// let options = Some(ListImagesOptions{
    ///   all: true,
    ///   filters,
    ///   ..Default::default()
    /// });
    ///
    /// docker.list_images(options);
    /// ```
    pub async fn list_images(
        &self,
        options: Option<impl Into<crate::query_parameters::ListImagesOptions>>,
    ) -> Result<Vec<ImageSummary>, Error> {
        let url = "/images/json";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Create Image
    ///
    /// Create an image by either pulling it from a registry or importing it.
    ///
    /// # Arguments
    ///
    ///  - An optional [Create Image Options](CreateImageOptions) struct.
    ///  - An optional request body consisting of a tar or tar.gz archive, or a stream
    ///    containing the root file system for the image. If this argument is used,
    ///    the value of the `from_src` option must be "-".
    ///
    /// # Returns
    ///
    ///  - [Create Image Info](CreateImageInfo), wrapped in an asynchronous
    ///    Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::CreateImageOptions;
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(CreateImageOptions{
    ///   from_image: "hello-world",
    ///   ..Default::default()
    /// });
    ///
    /// docker.create_image(options, None, None);
    ///
    /// // do some other work while the image is pulled from the docker hub...
    /// ```
    ///
    /// # Unsupported
    ///
    ///  - Import from tarball
    ///
    pub fn create_image(
        &self,
        options: Option<crate::query_parameters::CreateImageOptions>,
        root_fs: Option<BodyType>,
        credentials: Option<DockerCredentials>,
    ) -> impl Stream<Item = Result<CreateImageInfo, Error>> {
        let url = "/images/create";

        let req = self.build_request_with_registry_auth(
            url,
            Builder::new().method(Method::POST),
            options,
            Ok(root_fs.unwrap_or(BodyType::Left(Full::new(Bytes::new())))),
            DockerCredentialsHeader::Auth(credentials),
        );

        self.process_into_stream(req).boxed().map(|res| {
            if let Ok(CreateImageInfo {
                error: Some(error), ..
            }) = res
            {
                Err(Error::DockerStreamError { error })
            } else {
                res
            }
        })
    }

    /// ---
    ///
    /// # Inspect Image
    ///
    /// Return low-level information about an image.
    ///
    /// # Arguments
    ///
    /// - Image name as a string slice.
    ///
    /// # Returns
    ///
    ///  - [ImageInspect](ImageInspect), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use std::default::Default;
    ///
    /// docker.inspect_image("hello-world");
    /// ```
    pub async fn inspect_image(&self, image_name: &str) -> Result<ImageInspect, Error> {
        let url = format!("/images/{image_name}/json");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Inspect an Image by contacting the registry
    ///
    /// Return image digest and platform information by contacting the registry
    ///
    /// # Arguments
    ///
    /// - Image name as a string slice.
    ///
    /// # Returns
    ///
    /// - [DistributionInspect](DistributionInspect), wrapped in a Future
    ///
    /// # Examples
    /// ```rust
    /// use bollard::Docker;
    /// let docker = Docker::connect_with_http_defaults().unwrap();
    /// docker.inspect_registry_image("ubuntu:jammy", None);
    /// ```
    pub async fn inspect_registry_image(
        &self,
        image_name: &str,
        credentials: Option<DockerCredentials>,
    ) -> Result<DistributionInspect, Error> {
        let url = format!("/distribution/{image_name}/json");

        let req = self.build_request_with_registry_auth(
            &url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
            DockerCredentialsHeader::Auth(credentials),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Prune Images
    ///
    /// Delete unused images.
    ///
    /// # Arguments
    ///
    /// - An optional [Prune Images Options](PruneImagesOptions) struct.
    ///
    /// # Returns
    ///
    ///  - a [Prune Image Response](ImagePruneResponse), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::PruneImagesOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("until", vec!["10m"]);
    ///
    /// let options = Some(PruneImagesOptions {
    ///   filters
    /// });
    ///
    /// docker.prune_images(options);
    /// ```
    pub async fn prune_images(
        &self,
        options: Option<crate::query_parameters::PruneImagesOptions>,
    ) -> Result<ImagePruneResponse, Error> {
        let url = "/images/prune";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Image History
    ///
    /// Return parent layers of an image.
    ///
    /// # Arguments
    ///
    ///  - Image name as a string slice.
    ///
    /// # Returns
    ///
    ///  - Vector of [History Response Item](HistoryResponseItem), wrapped in a
    ///    Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.image_history("hello-world");
    /// ```
    pub async fn image_history(&self, image_name: &str) -> Result<Vec<HistoryResponseItem>, Error> {
        let url = format!("/images/{image_name}/history");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Search Images
    ///
    /// Search for an image on Docker Hub.
    ///
    /// # Arguments
    ///
    ///  - [Search Image Options](SearchImagesOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [Image Search Response Item](ImageSearchResponseItem) results, wrapped in a
    ///    Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    ///
    /// use bollard::image::SearchImagesOptions;
    /// use std::default::Default;
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("until", vec!["10m"]);
    ///
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let search_options = SearchImagesOptions {
    ///     term: "hello-world",
    ///     filters,
    ///     ..Default::default()
    /// };
    ///
    /// docker.search_images(search_options);
    /// ```
    pub async fn search_images(
        &self,
        options: crate::query_parameters::SearchImagesOptions,
    ) -> Result<Vec<ImageSearchResponseItem>, Error> {
        let url = "/images/search";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            Some(options),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Remove Image
    ///
    /// Remove an image, along with any untagged parent images that were referenced by that image.
    ///
    /// # Arguments
    ///
    ///  - Image name as a string slice.
    ///  - An optional [Remove Image Options](RemoveImageOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [Image Delete Response Item](ImageDeleteResponseItem), wrapped in a
    ///    Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    ///
    /// use bollard::image::RemoveImageOptions;
    /// use std::default::Default;
    ///
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let remove_options = Some(RemoveImageOptions {
    ///     force: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.remove_image("hello-world", remove_options, None);
    /// ```
    pub async fn remove_image(
        &self,
        image_name: &str,
        options: Option<crate::query_parameters::RemoveImageOptions>,
        credentials: Option<DockerCredentials>,
    ) -> Result<Vec<ImageDeleteResponseItem>, Error> {
        let url = format!("/images/{image_name}");

        let req = self.build_request_with_registry_auth(
            &url,
            Builder::new().method(Method::DELETE),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
            DockerCredentialsHeader::Auth(credentials),
        );
        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Tag Image
    ///
    /// Tag an image so that it becomes part of a repository.
    ///
    /// # Arguments
    ///
    ///  - Image name as a string slice.
    ///  - Optional [Tag Image Options](TagImageOptions) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    ///
    /// use bollard::image::TagImageOptions;
    /// use std::default::Default;
    ///
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let tag_options = Some(TagImageOptions {
    ///     tag: "v1.0.1",
    ///     ..Default::default()
    /// });
    ///
    /// docker.tag_image("hello-world", tag_options);
    /// ```
    pub async fn tag_image(
        &self,
        image_name: &str,
        options: Option<crate::query_parameters::TagImageOptions>,
    ) -> Result<(), Error> {
        let url = format!("/images/{image_name}/tag");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Push Image
    ///
    /// Push an image to a registry.
    ///
    /// # Arguments
    ///
    ///  - Image name as a string slice.
    ///  - Optional [Push Image Options](PushImageOptions) struct.
    ///  - Optional [Docker Credentials](DockerCredentials) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    ///
    /// use bollard::auth::DockerCredentials;
    /// use bollard::image::PushImageOptions;
    ///
    /// use std::default::Default;
    ///
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let push_options = Some(PushImageOptions {
    ///     tag: "v1.0.1",
    /// });
    ///
    /// let credentials = Some(DockerCredentials {
    ///     username: Some("Jack".to_string()),
    ///     password: Some("myverysecretpassword".to_string()),
    ///     ..Default::default()
    /// });
    ///
    /// docker.push_image("hello-world", push_options, credentials);
    /// ```
    pub fn push_image(
        &self,
        image_name: &str,
        options: Option<crate::query_parameters::PushImageOptions>,
        credentials: Option<DockerCredentials>,
    ) -> impl Stream<Item = Result<PushImageInfo, Error>> {
        let url = format!("/images/{image_name}/push");

        let req = self.build_request_with_registry_auth(
            &url,
            Builder::new()
                .method(Method::POST)
                .header(CONTENT_TYPE, "application/json"),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
            DockerCredentialsHeader::Auth(Some(credentials.unwrap_or_default())),
        );

        self.process_into_stream(req).boxed().map(|res| {
            if let Ok(PushImageInfo {
                error: Some(error), ..
            }) = res
            {
                Err(Error::DockerStreamError { error })
            } else {
                res
            }
        })
    }

    /// ---
    ///
    /// # Commit Container
    ///
    /// Create a new image from a container.
    ///
    /// # Arguments
    ///
    ///  - [Commit Container Options](CommitContainerOptions) struct.
    ///  - Container [Config](Config) struct.
    ///
    /// # Returns
    ///
    ///  - [Commit](Commit), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::CommitContainerOptions;
    /// use bollard::container::Config;
    ///
    /// use std::default::Default;
    ///
    /// let options = CommitContainerOptions{
    ///     container: "my-running-container",
    ///     pause: true,
    ///     ..Default::default()
    /// };
    ///
    /// let config = Config::<String> {
    ///     ..Default::default()
    /// };
    ///
    /// docker.commit_container(options, config);
    /// ```
    pub async fn commit_container(
        &self,
        options: crate::query_parameters::CommitContainerOptions,
        config: crate::models::ContainerConfig,
    ) -> Result<Commit, Error> {
        let url = "/commit";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            Some(options),
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Build Image
    ///
    /// Build an image from a tar archive with a `Dockerfile` in it.
    ///
    /// The `Dockerfile` specifies how the image is built from the tar archive. It is typically in
    /// the archive's root, but can be at a different path or have a different name by specifying
    /// the `dockerfile` parameter.
    ///
    /// By default, the call to build specifies using BuilderV1, the first generation builder in docker daemon.
    ///
    /// # Arguments
    ///
    ///  - [Build Image Options](BuildImageOptions) struct.
    ///  - Optional [Docker Credentials](DockerCredentials) struct.
    ///  - Tar archive compressed with one of the following algorithms: identity (no compression),
    ///    gzip, bzip2, xz. Optional [Hyper Body](hyper::body::Body).
    ///
    /// # Returns
    ///
    ///  - [Create Image Info](CreateImageInfo), wrapped in an asynchronous
    ///    Stream.
    ///
    /// # Examples
    ///
    /// Sending a tarball:
    ///
    /// ```rust,no_run
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::BuildImageOptions;
    /// use bollard::container::Config;
    /// use bollard::body_full;
    ///
    /// use std::default::Default;
    /// use std::fs::File;
    /// use std::io::Read;
    ///
    /// let options = BuildImageOptions{
    ///     dockerfile: "Dockerfile",
    ///     t: "my-image",
    ///     rm: true,
    ///     ..Default::default()
    /// };
    ///
    /// let mut file = File::open("tarball.tar.gz").unwrap();
    /// let mut contents = Vec::new();
    /// file.read_to_end(&mut contents).unwrap();
    ///
    /// docker.build_image(options, None, Some(body_full(contents.into())));
    /// ```
    ///
    /// Sending a stream:
    ///
    /// ```rust,no_run
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::BuildImageOptions;
    /// use bollard::container::Config;
    /// use bollard::body_stream;
    ///
    /// use std::default::Default;
    /// use std::fs::File;
    /// use std::io::Read;
    ///
    /// let options = BuildImageOptions{
    ///     dockerfile: "Dockerfile",
    ///     t: "my-image",
    ///     rm: true,
    ///     ..Default::default()
    /// };
    ///
    /// # let mut file = File::open("tarball.tar.gz").unwrap();
    /// # let mut contents = Vec::new();
    /// # file.read_to_end(&mut contents).unwrap();
    /// # let payload = Box::new(contents).leak();
    /// # let payload = payload.chunks(32);
    /// # let stream = futures_util::stream::iter(payload.map(bytes::Bytes::from));
    ///
    /// docker.build_image(options, None, Some(body_stream(stream)));
    /// ```
    pub fn build_image(
        &self,
        options: crate::query_parameters::BuildImageOptions,
        credentials: Option<HashMap<String, DockerCredentials>>,
        tar: Option<BodyType>,
    ) -> impl Stream<Item = Result<BuildInfo, Error>> + '_ {
        let url = "/build";

        match (
            if cfg!(feature = "buildkit")
                && options.version == crate::query_parameters::BuilderVersion::BuilderBuildKit
            {
                ImageBuildBuildkitEither::Left(credentials)
            } else {
                ImageBuildBuildkitEither::Right(credentials)
            },
            &options,
        ) {
            #[cfg(feature = "buildkit")]
            (
                ImageBuildBuildkitEither::Left(creds),
                crate::query_parameters::BuildImageOptions {
                    session: Some(sess),
                    ..
                },
            ) => {
                let session_id = String::clone(sess);
                let outputs = options.outputs.clone();

                let req = self.build_request(
                    url,
                    Builder::new()
                        .method(Method::POST)
                        .header(CONTENT_TYPE, "application/x-tar"),
                    Some(options),
                    Ok(tar.unwrap()),
                );

                let session = stream::once(
                    self.start_session(session_id, creds, outputs)
                        .map(|_| Either::Right(()))
                        .fuse(),
                );

                let stream = self.process_into_stream::<BuildInfo>(req).map(Either::Left);

                stream::select(stream, session)
                    .filter_map(|either| async move {
                        match either {
                            Either::Left(data) => Some(data),
                            _ => None,
                        }
                    })
                    .boxed()
            }
            #[cfg(feature = "buildkit")]
            (
                ImageBuildBuildkitEither::Left(_),
                crate::query_parameters::BuildImageOptions { session: None, .. },
            ) => stream::once(futures_util::future::err(
                Error::MissingSessionBuildkitError {},
            ))
            .boxed(),
            #[cfg(not(feature = "buildkit"))]
            (ImageBuildBuildkitEither::Left(_), _) => unimplemented!(
                "a buildkit enabled build without the 'buildkit' feature should not be possible"
            ),
            (ImageBuildBuildkitEither::Right(creds), _) => {
                let req = self.build_request_with_registry_auth(
                    url,
                    Builder::new()
                        .method(Method::POST)
                        .header(CONTENT_TYPE, "application/x-tar"),
                    Some(options),
                    Ok(tar.unwrap()),
                    DockerCredentialsHeader::Config(creds),
                );

                self.process_into_stream(req).boxed()
            }
        }
        .map(|res| {
            if let Ok(BuildInfo {
                error: Some(error), ..
            }) = res
            {
                Err(Error::DockerStreamError { error })
            } else {
                res
            }
        })
    }

    #[cfg(feature = "buildkit")]
    async fn start_session(
        &self,
        id: String,
        credentials: Option<HashMap<String, DockerCredentials>>,
        outputs: Option<crate::query_parameters::ImageBuildOutput>,
    ) -> Result<(), crate::grpc::error::GrpcError> {
        let driver = crate::grpc::driver::moby::Moby::new(self);

        let mut auth_provider = crate::grpc::AuthProvider::new();
        if let Some(creds) = credentials {
            for (host, docker_credentials) in creds {
                auth_provider.set_docker_credentials(&host, docker_credentials);
            }
        }

        let auth =
            bollard_buildkit_proto::moby::filesync::v1::auth_server::AuthServer::new(auth_provider);

        let mut services = match outputs {
            Some(crate::query_parameters::ImageBuildOutput::Tar(path)) => {
                let filesend_impl =
                    crate::grpc::FileSendImpl::new(std::path::PathBuf::from(path).as_path());
                let filesend =
                    bollard_buildkit_proto::moby::filesync::v1::file_send_server::FileSendServer::new(
                        filesend_impl,
                    );
                vec![crate::grpc::GrpcServer::FileSend(filesend)]
            }
            Some(crate::query_parameters::ImageBuildOutput::Local(path)) => {
                let filesendpacket_impl =
                    crate::grpc::FileSendPacketImpl::new(std::path::PathBuf::from(path).as_path());
                let filesendpacket = FileSendPacketServer::new(filesendpacket_impl);
                vec![crate::grpc::GrpcServer::FileSendPacket(filesendpacket)]
            }
            None => vec![],
        };

        services.push(crate::grpc::GrpcServer::Auth(auth));

        crate::grpc::driver::Driver::grpc_handle(driver, &id, services).await?;

        Ok(())
    }

    /// ---
    ///
    /// # Prune Build
    ///
    /// Delete contents of the build cache
    ///
    /// # Arguments
    ///
    /// - An optional [Prune Build Options](PruneBuildOptions) struct.
    ///
    /// # Returns
    ///
    ///  - a [Prune Build Response](BuildPruneResponse), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::PruneBuildOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("until", vec!["10m"]);
    ///
    /// let options = Some(PruneBuildOptions {
    ///   filters,
    ///   ..Default::default()
    /// });
    ///
    /// docker.prune_build(options);
    /// ```
    pub async fn prune_build(
        &self,
        options: Option<crate::query_parameters::PruneBuildOptions>,
    ) -> Result<BuildPruneResponse, Error> {
        let url = "/build/prune";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Export Image
    ///
    /// Get a tarball containing all images and metadata for a repository.
    ///
    /// The root of the resulting tar file will contain the file "manifest.json". If the export is
    /// of an image repository, rather than a single image, there will also be a `repositories` file
    /// with a JSON description of the exported image repositories.
    /// Additionally, each layer of all exported images will have a sub directory in the archive
    /// containing the filesystem of the layer.
    ///
    /// See the [Docker API documentation](https://docs.docker.com/engine/api/v1.40/#operation/ImageGet)
    /// for more information.
    /// # Arguments
    /// - The `image_name` string referring to an individual image and tag (e.g. alpine:latest)
    ///
    /// # Returns
    ///  - An uncompressed TAR archive
    pub fn export_image(&self, image_name: &str) -> impl Stream<Item = Result<Bytes, Error>> {
        let url = format!("/images/{image_name}/get");
        let req = self.build_request(
            &url,
            Builder::new()
                .method(Method::GET)
                .header(CONTENT_TYPE, "application/json"),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );
        self.process_into_body(req)
    }

    /// ---
    ///
    /// # Export Images
    ///
    /// Get a tarball containing all images and metadata for several image repositories. Shared
    /// layers will be deduplicated.
    ///
    /// See the [Docker API documentation](https://docs.docker.com/engine/api/v1.40/#tag/Image/operation/ImageGetAll)
    /// for more information.
    /// # Arguments
    /// - The `image_names` Vec of image names.
    ///
    /// # Returns
    ///  - An uncompressed TAR archive
    pub fn export_images(&self, image_names: &[&str]) -> impl Stream<Item = Result<Bytes, Error>> {
        let options: Vec<_> = image_names.iter().map(|name| ("names", name)).collect();
        let req = self.build_request(
            "/images/get",
            Builder::new()
                .method(Method::GET)
                .header(CONTENT_TYPE, "application/json"),
            Some(options),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );
        self.process_into_body(req)
    }

    /// ---
    ///
    /// # Import Image
    ///
    /// Load a set of images and tags into a repository.
    ///
    /// For details on the format, see the [export image
    /// endpoint](struct.Docker.html#method.export_image).
    ///
    /// # Arguments
    ///  - [Image Import Options](ImportImageOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [Build Info](BuildInfo), wrapped in an asynchronous
    ///    Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::ImportImageOptions;
    /// use bollard::errors::Error;
    /// use bollard::body_full;
    ///
    /// use std::default::Default;
    /// use futures_util::stream::{StreamExt, TryStreamExt};
    /// use tokio::fs::File;
    /// use tokio::io::AsyncWriteExt;
    /// use tokio_util::codec;
    ///
    /// let options = ImportImageOptions{
    ///     ..Default::default()
    /// };
    ///
    /// async move {
    ///     let mut file = File::open("tarball.tar.gz").await.unwrap();
    ///
    ///     let mut byte_stream = codec::FramedRead::new(file, codec::BytesCodec::new()).map(|r| {
    ///         let bytes = r.unwrap().freeze();
    ///         Ok::<_, Error>(bytes)
    ///     });
    ///
    ///     let bytes = byte_stream.next().await.unwrap().unwrap();
    ///
    ///     let mut stream = docker
    ///         .import_image(
    ///             ImportImageOptions {
    ///                 ..Default::default()
    ///             },
    ///             body_full(bytes),
    ///             None,
    ///         );
    ///
    ///     while let Some(response) = stream.next().await {
    ///         // ...
    ///     }
    /// };
    /// ```
    pub fn import_image(
        &self,
        options: crate::query_parameters::ImportImageOptions,
        root_fs: BodyType,
        credentials: Option<HashMap<String, DockerCredentials>>,
    ) -> impl Stream<Item = Result<BuildInfo, Error>> {
        let req = self.build_request_with_registry_auth(
            "/images/load",
            Builder::new()
                .method(Method::POST)
                .header(CONTENT_TYPE, "application/x-tar"),
            Some(options),
            Ok(root_fs),
            DockerCredentialsHeader::Config(credentials),
        );

        self.process_into_stream(req).boxed().map(|res| {
            if let Ok(BuildInfo {
                error: Some(error), ..
            }) = res
            {
                Err(Error::DockerStreamError { error })
            } else {
                res
            }
        })
    }

    /// ---
    ///
    /// # Import Image (stream)
    ///
    /// Load a set of images and tags into a repository, without holding it all in memory at a given point in time
    ///
    /// For details on the format, see the [export image
    /// endpoint](struct.Docker.html#method.export_image).
    ///
    /// # Arguments
    ///  - [Image Import Options](ImportImageOptions) struct.
    ///  - Stream producing `Bytes` of the image
    ///
    /// # Returns
    ///
    ///  - [Build Info](BuildInfo), wrapped in an asynchronous
    ///    Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::ImportImageOptions;
    /// use bollard::errors::Error;
    ///
    /// use std::default::Default;
    /// use futures_util::stream::{StreamExt, TryStreamExt};
    /// use tokio::fs::File;
    /// use tokio::io::AsyncWriteExt;
    /// use tokio_util::codec;
    ///
    /// let options = ImportImageOptions{
    ///     ..Default::default()
    /// };
    ///
    /// async move {
    ///     let mut file = File::open("tarball.tar.gz").await.unwrap();
    ///
    ///     let mut byte_stream = codec::FramedRead::new(file, codec::BytesCodec::new()).map(|r| {
    ///         r.unwrap().freeze()
    ///     });
    ///
    ///     let mut stream = docker
    ///         .import_image_stream(
    ///             ImportImageOptions {
    ///                 ..Default::default()
    ///             },
    ///             byte_stream,
    ///             None,
    ///         );
    ///
    ///     while let Some(response) = stream.next().await {
    ///         // ...
    ///     }
    /// };
    /// ```
    pub fn import_image_stream(
        &self,
        options: crate::query_parameters::ImportImageOptions,
        root_fs: impl Stream<Item = Bytes> + Send + 'static,
        credentials: Option<HashMap<String, DockerCredentials>>,
    ) -> impl Stream<Item = Result<BuildInfo, Error>> {
        let req = self.build_request_with_registry_auth(
            "/images/load",
            Builder::new()
                .method(Method::POST)
                .header(CONTENT_TYPE, "application/json"),
            Some(options),
            Ok(body_stream(root_fs)),
            DockerCredentialsHeader::Config(credentials),
        );

        self.process_into_stream(req).boxed().map(|res| {
            if let Ok(BuildInfo {
                error: Some(error), ..
            }) = res
            {
                Err(Error::DockerStreamError { error })
            } else {
                res
            }
        })
    }
}

#[cfg(not(windows))]
#[cfg(test)]
mod tests {

    use std::io::Write;

    use bollard_stubs::query_parameters::{BuildImageOptionsBuilder, CreateImageOptionsBuilder};
    use futures_util::TryStreamExt;
    use yup_hyper_mock::HostToReplyConnector;

    use crate::{Docker, API_DEFAULT_VERSION};

    #[tokio::test]
    async fn test_create_image_with_error() {
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            String::from("http://127.0.0.1"),
            "HTTP/1.1 200 OK\r\nServer:mock1\r\nContent-Type:application/json\r\n\r\n{\"status\":\"Pulling from localstack/localstack\",\"id\":\"0.14.2\"}\n{\"errorDetail\":{\"message\":\"Get \\\"[https://registry-1.docker.io/v2/localstack/localstack/manifests/sha256:d7aefdaae6712891f13795f538fd855fe4e5a8722249e9ca965e94b69b83b819](https://registry-1.docker.io/v2/localstack/localstack/manifests/sha256:d7aefdaae6712891f13795f538fd855fe4e5a8722249e9ca965e94b69b83b819/)\\\": EOF\"},\"error\":\"Get \\\"[https://registry-1.docker.io/v2/localstack/localstack/manifests/sha256:d7aefdaae6712891f13795f538fd855fe4e5a8722249e9ca965e94b69b83b819](https://registry-1.docker.io/v2/localstack/localstack/manifests/sha256:d7aefdaae6712891f13795f538fd855fe4e5a8722249e9ca965e94b69b83b819/)\\\": EOF\"}".to_string());

        let docker =
            Docker::connect_with_mock(connector, "127.0.0.1".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let image = String::from("localstack");

        let result = &docker
            .create_image(
                Some(
                    CreateImageOptionsBuilder::default()
                        .from_image(&image)
                        .build(),
                ),
                None,
                None,
            )
            .try_collect::<Vec<_>>()
            .await;

        assert!(matches!(
            result,
            Err(crate::errors::Error::DockerStreamError { error: _ })
        ));
    }

    #[tokio::test]
    async fn test_push_image_with_error() {
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            String::from("http://127.0.0.1"),
            "HTTP/1.1 200 OK\r\nServer:mock1\r\nContent-Type:application/json\r\n\r\n{\"status\":\"The push refers to repository [localhost:5000/centos]\"}\n{\"status\":\"Preparing\",\"progressDetail\":{},\"id\":\"74ddd0ec08fa\"}\n{\"errorDetail\":{\"message\":\"EOF\"},\"error\":\"EOF\"}".to_string());

        let docker =
            Docker::connect_with_mock(connector, "127.0.0.1".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let image = String::from("centos");

        let result = docker
            .push_image(&image[..], None, None)
            .try_collect::<Vec<_>>()
            .await;

        assert!(matches!(
            result,
            Err(crate::errors::Error::DockerStreamError { error: _ })
        ));
    }

    #[tokio::test]
    async fn test_build_image_with_error() {
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            String::from("http://127.0.0.1"),
            "HTTP/1.1 200 OK\r\nServer:mock1\r\nContent-Type:application/json\r\n\r\n{\"stream\":\"Step 1/2 : FROM alpine\"}\n{\"stream\":\"\n\"}\n{\"status\":\"Pulling from library/alpine\",\"id\":\"latest\"}\n{\"status\":\"Digest: sha256:bc41182d7ef5ffc53a40b044e725193bc10142a1243f395ee852a8d9730fc2ad\"}\n{\"status\":\"Status: Image is up to date for alpine:latest\"}\n{\"stream\":\" --- 9c6f07244728\\n\"}\n{\"stream\":\"Step 2/2 : RUN cmd.exe /C copy nul bollard.txt\"}\n{\"stream\":\"\\n\"}\n{\"stream\":\" --- Running in d615794caf91\\n\"}\n{\"stream\":\"/bin/sh: cmd.exe: not found\\n\"}\n{\"errorDetail\":{\"code\":127,\"message\":\"The command '/bin/sh -c cmd.exe /C copy nul bollard.txt' returned a non-zero code: 127\"},\"error\":\"The command '/bin/sh -c cmd.exe /C copy nul bollard.txt' returned a non-zero code: 127\"}".to_string());
        let docker =
            Docker::connect_with_mock(connector, "127.0.0.1".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let dockerfile = String::from(
            r#"FROM alpine
            RUN cmd.exe /C copy nul bollard.txt"#,
        );

        let mut header = tar::Header::new_gnu();
        header.set_path("Dockerfile").unwrap();
        header.set_size(dockerfile.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        let mut tar = tar::Builder::new(Vec::new());
        tar.append(&header, dockerfile.as_bytes()).unwrap();

        let uncompressed = tar.into_inner().unwrap();
        let mut c = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        c.write_all(&uncompressed).unwrap();
        let compressed = c.finish().unwrap();

        let result = &docker
            .build_image(
                BuildImageOptionsBuilder::default()
                    .dockerfile("Dockerfile")
                    .t("integration_test_build_image")
                    .pull("true")
                    .rm(true)
                    .build(),
                None,
                Some(http_body_util::Either::Left(compressed.into())),
            )
            .try_collect::<Vec<_>>()
            .await;

        println!("{result:#?}");

        assert!(matches!(
            result,
            Err(crate::errors::Error::DockerStreamError { error: _ })
        ));
    }
}
