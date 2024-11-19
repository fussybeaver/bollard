//! Image API: creating, manipulating and pushing docker images
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
use serde::Serialize;
use serde_repr::*;

use super::Docker;
use crate::auth::{DockerCredentials, DockerCredentialsHeader};
use crate::container::Config;
use crate::docker::{body_stream, BodyType};
use crate::errors::Error;
use crate::models::*;

use std::cmp::Eq;
use std::collections::HashMap;
use std::hash::Hash;

/// Parameters available for pulling an image, used in the [Create Image
/// API](Docker::create_image)
///
/// ## Examples
///
/// ```rust
/// use bollard::image::CreateImageOptions;
///
/// use std::default::Default;
///
/// CreateImageOptions{
///   from_image: "hello-world",
///   ..Default::default()
/// };
/// ```
///
/// ```rust
/// # use bollard::image::CreateImageOptions;
/// # use std::default::Default;
/// CreateImageOptions::<String>{
///   ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateImageOptions<'a, T>
where
    T: Into<String> + Serialize,
{
    /// Name of the image to pull. The name may include a tag or digest. This parameter may only be
    /// used when pulling an image. The pull is cancelled if the HTTP connection is closed.
    pub from_image: T,
    /// Source to import. The value may be a URL from which the image can be retrieved or `-` to
    /// read the image from the request body. This parameter may only be used when importing an
    /// image.
    pub from_src: T,
    /// Repository name given to an image when it is imported. The repo may include a tag. This
    /// parameter may only be used when importing an image.
    pub repo: T,
    /// Tag or digest. If empty when pulling an image, this causes all tags for the given image to
    /// be pulled.
    pub tag: T,
    /// Platform in the format `os[/arch[/variant]]`
    pub platform: T,
    /// A list of Dockerfile instructions to be applied to the image being created. Changes must be
    /// URL-encoded! This parameter may only be used when importing an image.
    #[serde(
        serialize_with = "crate::docker::serialize_join_newlines",
        skip_serializing_if = "Vec::is_empty" // if an empty changes parameter is sent, Docker returns a 400 "file with no instructions" error
    )]
    pub changes: Vec<&'a str>,
}

/// Parameters to the [List Images
/// API](Docker::list_images())
///
/// ## Examples
///
/// ```rust
/// use bollard::image::ListImagesOptions;
///
/// use std::collections::HashMap;
/// use std::default::Default;
///
/// let mut filters = HashMap::new();
/// filters.insert("dangling", vec!["true"]);
///
/// ListImagesOptions{
///   all: true,
///   filters,
///   ..Default::default()
/// };
/// ```
///
/// ```rust
/// # use bollard::image::ListImagesOptions;
/// # use std::default::Default;
/// ListImagesOptions::<String>{
///   ..Default::default()
/// };
/// ```
///
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ListImagesOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
{
    /// Show all images. Only images from a final layer (no children) are shown by default.
    pub all: bool,
    /// A JSON encoded value of the filters to process on the images list. Available filters:
    ///  - `before`=(`<image-name>[:<tag>]`, `<image id>` or `<image@digest>`)
    ///  - `dangling`=`true`
    ///  - `label`=`key` or `label`=`"key=value"` of an image label
    ///  - `reference`=(`<image-name>[:<tag>]`)
    ///  - `since`=(`<image-name>[:<tag>]`, `<image id>` or `<image@digest>`)
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
    /// Show digest information as a RepoDigests field on each image.
    pub digests: bool,
}

/// Parameters to the [Prune Images API](Docker::prune_images())
///
/// ## Examples
///
/// ```rust
/// use bollard::image::PruneImagesOptions;
///
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("until", vec!["10m"]);
///
/// PruneImagesOptions{
///   filters,
/// };
/// ```
///
/// ```rust
/// # use bollard::image::PruneImagesOptions;
/// # use std::default::Default;
/// PruneImagesOptions::<String>{
///   ..Default::default()
/// };
/// ```
///
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PruneImagesOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
{
    /// Filters to process on the prune list, encoded as JSON. Available filters:
    ///  - `dangling=<boolean>` When set to `true` (or `1`), prune only unused *and* untagged
    ///    images. When set to `false` (or `0`), all unused images are pruned.
    ///  - `until=<string>` Prune images created before this timestamp. The `<timestamp>` can be
    ///    Unix timestamps, date formatted timestamps, or Go duration strings (e.g. `10m`, `1h30m`)
    ///    computed relative to the daemon machine’s time.
    ///  - `label` (`label=<key>`, `label=<key>=<value>`, `label!=<key>`, or
    ///    `label!=<key>=<value>`) Prune images with (or without, in case `label!=...` is used) the
    ///    specified labels.
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

/// Parameters to the [Search Images API](Docker::search_images())
///
/// ## Example
///
/// ```rust
/// use bollard::image::SearchImagesOptions;
/// use std::default::Default;
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("until", vec!["10m"]);
///
/// SearchImagesOptions {
///     term: "hello-world",
///     filters,
///     ..Default::default()
/// };
/// ```
///
/// ```rust
/// # use bollard::image::SearchImagesOptions;
/// # use std::default::Default;
/// SearchImagesOptions::<String> {
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SearchImagesOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
{
    /// Term to search (required)
    pub term: T,
    /// Maximum number of results to return
    pub limit: Option<u64>,
    /// A JSON encoded value of the filters to process on the images list. Available filters:
    ///  - `is-automated=(true|false)`
    ///  - `is-official=(true|false)`
    ///  - `stars=<number>` Matches images that has at least 'number' stars.
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

/// Parameters to the [Remove Image API](Docker::remove_image())
///
/// ## Examples
///
/// ```rust
/// use bollard::image::RemoveImageOptions;
/// use std::default::Default;
///
/// RemoveImageOptions {
///     force: true,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
pub struct RemoveImageOptions {
    /// Remove the image even if it is being used by stopped containers or has other tags.
    pub force: bool,
    /// Do not delete untagged parent images.
    pub noprune: bool,
}

/// Parameters to the [Tag Image API](Docker::tag_image())
///
/// ## Examples
///
/// ```rust
/// use bollard::image::TagImageOptions;
/// use std::default::Default;
///
/// let tag_options = TagImageOptions {
///     tag: "v1.0.1",
///     ..Default::default()
/// };
/// ```
///
/// ```rust
/// # use bollard::image::TagImageOptions;
/// # use std::default::Default;
/// let tag_options = TagImageOptions::<String> {
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TagImageOptions<T>
where
    T: Into<String> + Serialize,
{
    /// The repository to tag in. For example, `someuser/someimage`.
    pub repo: T,
    /// The name of the new tag.
    pub tag: T,
}

/// Parameters to the [Push Image API](Docker::push_image())
///
/// ## Examples
///
/// ```rust
/// use bollard::image::PushImageOptions;
///
/// PushImageOptions {
///     tag: "v1.0.1",
/// };
/// ```
///
/// ```
/// # use bollard::image::PushImageOptions;
/// # use std::default::Default;
/// PushImageOptions::<String> {
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PushImageOptions<T>
where
    T: Into<String> + Serialize,
{
    /// The tag to associate with the image on the registry.
    pub tag: T,
}

/// Parameters to the [Commit Container API](Docker::commit_container())
///
/// ## Examples
///
/// ```rust
/// use bollard::image::CommitContainerOptions;
///
/// CommitContainerOptions {
///     container: "my-running-container",
///     pause: true,
///     ..Default::default()
/// };
/// ```
///
/// ```
/// # use bollard::image::CommitContainerOptions;
/// # use std::default::Default;
/// CommitContainerOptions::<String> {
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CommitContainerOptions<T>
where
    T: Into<String> + Serialize,
{
    /// The ID or name of the container to commit.
    pub container: T,
    /// Repository name for the created image.
    pub repo: T,
    /// Tag name for the create image.
    pub tag: T,
    /// Commit message.
    pub comment: T,
    /// Author of the image.
    pub author: T,
    /// Whether to pause the container before committing.
    pub pause: bool,
    /// `Dockerfile` instructions to apply while committing
    pub changes: Option<T>,
}

/// Parameters to the [Build Image API](Docker::build_image())
///
/// ## Examples
///
/// ```rust
/// use bollard::image::BuildImageOptions;
///
/// BuildImageOptions {
///     dockerfile: "Dockerfile",
///     t: "my-image",
///     ..Default::default()
/// };
/// ```
///
/// ```
/// # use bollard::image::BuildImageOptions;
/// # use std::default::Default;
/// BuildImageOptions::<String> {
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BuildImageOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
{
    /// Path within the build context to the `Dockerfile`. This is ignored if `remote` is specified and
    /// points to an external `Dockerfile`.
    pub dockerfile: T,
    /// A name and optional tag to apply to the image in the `name:tag` format. If you omit the tag
    /// the default `latest` value is assumed. You can provide several `t` parameters.
    pub t: T,
    /// Extra hosts to add to `/etc/hosts`.
    pub extrahosts: Option<T>,
    /// A Git repository URI or HTTP/HTTPS context URI. If the URI points to a single text file,
    /// the file’s contents are placed into a file called `Dockerfile` and the image is built from
    /// that file. If the URI points to a tarball, the file is downloaded by the daemon and the
    /// contents therein used as the context for the build. If the URI points to a tarball and the
    /// `dockerfile` parameter is also specified, there must be a file with the corresponding path
    /// inside the tarball.
    pub remote: T,
    /// Suppress verbose build output.
    pub q: bool,
    /// Do not use the cache when building the image.
    pub nocache: bool,
    /// JSON array of images used for build cache resolution.
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub cachefrom: Vec<T>,
    /// Attempt to pull the image even if an older image exists locally.
    pub pull: bool,
    /// Remove intermediate containers after a successful build.
    pub rm: bool,
    /// Always remove intermediate containers, even upon failure.
    pub forcerm: bool,
    /// Set memory limit for build.
    pub memory: Option<u64>,
    /// Total memory (memory + swap). Set as `-1` to disable swap.
    pub memswap: Option<i64>,
    /// CPU shares (relative weight).
    pub cpushares: Option<u64>,
    /// CPUs in which to allow execution (e.g., `0-3`, `0,1`).
    pub cpusetcpus: T,
    /// The length of a CPU period in microseconds.
    pub cpuperiod: Option<u64>,
    /// Microseconds of CPU time that the container can get in a CPU period.
    pub cpuquota: Option<u64>,
    /// JSON map of string pairs for build-time variables. Users pass these values at build-time.
    /// Docker uses the buildargs as the environment context for commands run via the `Dockerfile`
    /// RUN instruction, or for variable expansion in other `Dockerfile` instructions.
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub buildargs: HashMap<T, T>,
    #[cfg(feature = "buildkit")]
    /// Session ID
    pub session: Option<String>,
    /// Size of `/dev/shm` in bytes. The size must be greater than 0. If omitted the system uses 64MB.
    pub shmsize: Option<u64>,
    /// Squash the resulting images layers into a single layer.
    pub squash: bool,
    /// Arbitrary key/value labels to set on the image, as a JSON map of string pairs.
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub labels: HashMap<T, T>,
    /// Sets the networking mode for the run commands during build. Supported standard values are:
    /// `bridge`, `host`, `none`, and `container:<name|id>`. Any other value is taken as a custom network's
    /// name to which this container should connect to.
    pub networkmode: T,
    /// Platform in the format `os[/arch[/variant]]`
    pub platform: T,
    /// Target build stage
    pub target: T,
    #[cfg(feature = "buildkit")]
    /// Specify a custom exporter.
    pub outputs: Option<ImageBuildOutput<T>>,
    /// Builder version to use
    pub version: BuilderVersion,
}

#[cfg(feature = "buildkit")]
/// The exporter to use (see [Docker Docs](https://docs.docker.com/reference/cli/docker/buildx/build/#output))
#[derive(Debug, Clone, PartialEq)]
pub enum ImageBuildOutput<T>
where
    T: Into<String>,
{
    /// The local export type writes all result files to a directory on the client.
    /// The new files will be owned by the current user.
    /// On multi-platform builds, all results will be put in subdirectories by their platform.
    /// It takes the destination directory as a first argument.
    Tar(T),
    /// The tar export type writes all result files as a single tarball on the client.
    /// On multi-platform builds all results will be put in subdirectories by their platform.
    /// It takes the destination directory as a first argument.
    ///
    /// **Notice**: The implementation of the underlying `fsutil` protocol is not complete.
    /// Therefore, special files, permissions, etc. are ignored or not handled correctly.
    Local(T),
}

#[cfg(feature = "buildkit")]
impl<T> Serialize for ImageBuildOutput<T>
where
    T: Into<String>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ImageBuildOutput::Tar(_) => serializer.serialize_str(r#"[{"type": "tar"}]"#),
            ImageBuildOutput::Local(_) => serializer.serialize_str(r#"[{"type": "local"}]"#),
        }
    }
}

#[cfg(feature = "buildkit")]
impl<T> ImageBuildOutput<T>
where
    T: Into<String>,
{
    fn into_string(self) -> ImageBuildOutput<String> {
        match self {
            ImageBuildOutput::Tar(path) => ImageBuildOutput::Tar(path.into()),
            ImageBuildOutput::Local(path) => ImageBuildOutput::Local(path.into()),
        }
    }
}

/// Builder Version to use
#[derive(Clone, Copy, Debug, PartialEq, Serialize_repr)]
#[repr(u8)]
#[derive(Default)]
pub enum BuilderVersion {
    /// BuilderV1 is the first generation builder in docker daemon
    #[default]
    BuilderV1 = 1,
    /// BuilderBuildKit is builder based on moby/buildkit project
    BuilderBuildKit = 2,
}

enum ImageBuildBuildkitEither {
    #[allow(dead_code)]
    Left(Option<HashMap<String, DockerCredentials>>),
    Right(Option<HashMap<String, DockerCredentials>>),
}

/// Parameters to the [Import Image API](Docker::import_image())
///
/// ## Examples
///
/// ```rust
/// use bollard::image::ImportImageOptions;
/// use std::default::Default;
///
/// ImportImageOptions {
///     quiet: true,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
pub struct ImportImageOptions {
    /// Suppress progress details during load.
    pub quiet: bool,
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
    pub async fn list_images<T>(
        &self,
        options: Option<ListImagesOptions<T>>,
    ) -> Result<Vec<ImageSummary>, Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/images/json";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            options,
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
    ///  - An optional request body consisting of a tar or tar.gz archive with the root file system
    ///    for the image. If this argument is used, the value of the `from_src` option must be "-".
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
    pub fn create_image<T>(
        &self,
        options: Option<CreateImageOptions<'_, T>>,
        root_fs: Option<Bytes>,
        credentials: Option<DockerCredentials>,
    ) -> impl Stream<Item = Result<CreateImageInfo, Error>>
    where
        T: Into<String> + Serialize + std::fmt::Debug + Clone,
    {
        let url = "/images/create";

        let req = self.build_request_with_registry_auth(
            url,
            Builder::new().method(Method::POST),
            options,
            Ok(BodyType::Left(Full::new(match root_fs {
                Some(body) => body,
                None => Bytes::new(),
            }))),
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
    pub async fn prune_images<T>(
        &self,
        options: Option<PruneImagesOptions<T>>,
    ) -> Result<ImagePruneResponse, Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
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
    pub async fn search_images<T>(
        &self,
        options: SearchImagesOptions<T>,
    ) -> Result<Vec<ImageSearchResponseItem>, Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
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
        options: Option<RemoveImageOptions>,
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
    pub async fn tag_image<T>(
        &self,
        image_name: &str,
        options: Option<TagImageOptions<T>>,
    ) -> Result<(), Error>
    where
        T: Into<String> + Serialize,
    {
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
    pub fn push_image<T>(
        &self,
        image_name: &str,
        options: Option<PushImageOptions<T>>,
        credentials: Option<DockerCredentials>,
    ) -> impl Stream<Item = Result<PushImageInfo, Error>>
    where
        T: Into<String> + Serialize,
    {
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
    pub async fn commit_container<T, Z>(
        &self,
        options: CommitContainerOptions<T>,
        config: Config<Z>,
    ) -> Result<Commit, Error>
    where
        T: Into<String> + Serialize,
        Z: Into<String> + Eq + Hash + Serialize,
    {
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
    /// ```rust,no_run
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::BuildImageOptions;
    /// use bollard::container::Config;
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
    /// docker.build_image(options, None, Some(contents.into()));
    /// ```
    pub fn build_image<T>(
        &self,
        options: BuildImageOptions<T>,
        credentials: Option<HashMap<String, DockerCredentials>>,
        tar: Option<Bytes>,
    ) -> impl Stream<Item = Result<BuildInfo, Error>> + '_
    where
        T: Into<String> + Eq + Hash + Serialize + Clone,
    {
        let url = "/build";

        match (
            if cfg!(feature = "buildkit") && options.version == BuilderVersion::BuilderBuildKit {
                ImageBuildBuildkitEither::Left(credentials)
            } else {
                ImageBuildBuildkitEither::Right(credentials)
            },
            &options,
        ) {
            #[cfg(feature = "buildkit")]
            (
                ImageBuildBuildkitEither::Left(creds),
                BuildImageOptions {
                    session: Some(ref sess),
                    ..
                },
            ) => {
                let session_id = String::clone(sess);
                let outputs = options.outputs.clone().map(ImageBuildOutput::into_string);

                let req = self.build_request(
                    url,
                    Builder::new()
                        .method(Method::POST)
                        .header(CONTENT_TYPE, "application/x-tar"),
                    Some(options),
                    Ok(BodyType::Left(Full::new(tar.unwrap_or_default()))),
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
            (ImageBuildBuildkitEither::Left(_), BuildImageOptions { session: None, .. }) => {
                stream::once(futures_util::future::err(
                    Error::MissingSessionBuildkitError {},
                ))
                .boxed()
            }
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
                    Ok(BodyType::Left(Full::new(tar.unwrap_or_default()))),
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
        outputs: Option<ImageBuildOutput<String>>,
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
            Some(ImageBuildOutput::Tar(path)) => {
                let filesend_impl =
                    crate::grpc::FileSendImpl::new(std::path::PathBuf::from(path).as_path());
                let filesend =
                    bollard_buildkit_proto::moby::filesync::v1::file_send_server::FileSendServer::new(
                        filesend_impl,
                    );
                vec![crate::grpc::GrpcServer::FileSend(filesend)]
            }
            Some(ImageBuildOutput::Local(path)) => {
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
    ///             bytes,
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
        options: ImportImageOptions,
        root_fs: Bytes,
        credentials: Option<HashMap<String, DockerCredentials>>,
    ) -> impl Stream<Item = Result<BuildInfo, Error>> {
        let req = self.build_request_with_registry_auth(
            "/images/load",
            Builder::new()
                .method(Method::POST)
                .header(CONTENT_TYPE, "application/json"),
            Some(options),
            Ok(BodyType::Left(Full::new(root_fs))),
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
        options: ImportImageOptions,
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

    use futures_util::TryStreamExt;
    use yup_hyper_mock::HostToReplyConnector;

    use crate::{
        image::{BuildImageOptions, PushImageOptions},
        Docker, API_DEFAULT_VERSION,
    };

    use super::CreateImageOptions;

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
                Some(CreateImageOptions {
                    from_image: &image[..],
                    ..Default::default()
                }),
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
            .push_image(&image[..], None::<PushImageOptions<String>>, None)
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
                BuildImageOptions {
                    dockerfile: "Dockerfile".to_string(),
                    t: "integration_test_build_image".to_string(),
                    pull: true,
                    rm: true,
                    ..Default::default()
                },
                None,
                Some(compressed.into()),
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
