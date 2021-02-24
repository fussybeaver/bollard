//! Image API: creating, manipulating and pushing docker images
use futures_core::Stream;
use futures_util::{stream, stream::StreamExt};
use http::header::CONTENT_TYPE;
use http::request::Builder;
use hyper::{body::Bytes, Body, Method};
use serde::Serialize;

use super::Docker;
use crate::auth::DockerCredentials;
use crate::container::Config;
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
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateImageOptions<T>
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
#[derive(Debug, Clone, Default, Serialize)]
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct PruneImagesOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
{
    /// Filters to process on the prune list, encoded as JSON. Available filters:
    ///  - `dangling=<boolean>` When set to `true` (or `1`), prune only unused *and* untagged
    ///  images. When set to `false` (or `0`), all unused images are pruned.
    ///  - `until=<string>` Prune images created before this timestamp. The `<timestamp>` can be
    ///  Unix timestamps, date formatted timestamps, or Go duration strings (e.g. `10m`, `1h30m`)
    ///  computed relative to the daemon machine’s time.
    ///  - `label` (`label=<key>`, `label=<key>=<value>`, `label!=<key>`, or
    ///  `label!=<key>=<value>`) Prune images with (or without, in case `label!=...` is used) the
    ///  specified labels.
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
#[derive(Debug, Clone, Default, Serialize)]
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
#[derive(Debug, Copy, Clone, Default, Serialize)]
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
#[derive(Debug, Clone, Default, Serialize)]
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
#[derive(Debug, Clone, Default, Serialize)]
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
#[derive(Debug, Clone, Default, Serialize)]
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
#[derive(Debug, Clone, Default, Serialize)]
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
#[derive(Debug, Copy, Clone, Default, Serialize)]
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
            Ok(Body::empty()),
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
    ///  - [Build Info](BuildInfo), wrapped in an asynchronous
    ///  Stream.
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
        options: Option<CreateImageOptions<T>>,
        root_fs: Option<Body>,
        credentials: Option<DockerCredentials>,
    ) -> impl Stream<Item = Result<CreateImageInfo, Error>>
    where
        T: Into<String> + Serialize,
    {
        let url = "/images/create";

        match serde_json::to_string(&credentials.unwrap_or_else(|| DockerCredentials {
            ..Default::default()
        })) {
            Ok(ser_cred) => {
                let req = self.build_request(
                    url,
                    Builder::new()
                        .method(Method::POST)
                        .header("X-Registry-Auth", base64::encode(&ser_cred)),
                    options,
                    match root_fs {
                        Some(body) => Ok(body),
                        None => Ok(Body::empty()),
                    },
                );
                self.process_into_stream(req).boxed()
            }
            Err(e) => stream::once(async move { Err(Error::from(e)) }).boxed(),
        }
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
    ///  - [Image](Image), wrapped in a Future.
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
    pub async fn inspect_image(&self, image_name: &str) -> Result<Image, Error> {
        let url = format!("/images/{}/json", image_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
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
            Ok(Body::empty()),
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
    ///  Future.
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
        let url = format!("/images/{}/history", image_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
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
    ///  Future.
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
            Ok(Body::empty()),
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
    ///  Future.
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
        let url = format!("/images/{}", image_name);

        match serde_json::to_string(&credentials.unwrap_or_else(|| DockerCredentials {
            ..Default::default()
        })) {
            Ok(ser_cred) => {
                let req = self.build_request(
                    &url,
                    Builder::new()
                        .method(Method::DELETE)
                        .header("X-Registry-Auth", base64::encode(&ser_cred)),
                    options,
                    Ok(Body::empty()),
                );
                self.process_into_value(req).await
            }
            Err(e) => Err(e.into()),
        }
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
        let url = format!("/images/{}/tag", image_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options,
            Ok(Body::empty()),
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
        let url = format!("/images/{}/push", image_name);

        match serde_json::to_string(&credentials.unwrap_or_else(|| DockerCredentials {
            ..Default::default()
        })) {
            Ok(ser_cred) => {
                let req = self.build_request(
                    &url,
                    Builder::new()
                        .method(Method::POST)
                        .header(CONTENT_TYPE, "application/json")
                        .header("X-Registry-Auth", base64::encode(&ser_cred)),
                    options,
                    Ok(Body::empty()),
                );

                self.process_into_stream(req).boxed()
            }
            Err(e) => stream::once(async move { Err(e.into()) }).boxed(),
        }
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
    ///  Stream.
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
        tar: Option<Body>,
    ) -> impl Stream<Item = Result<BuildInfo, Error>>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/build";

        match serde_json::to_string(&credentials.unwrap_or_else(HashMap::new)) {
            Ok(ser_cred) => {
                let req = self.build_request(
                    &url,
                    Builder::new()
                        .method(Method::POST)
                        .header(CONTENT_TYPE, "application/x-tar")
                        .header("X-Registry-Config", base64::encode(&ser_cred)),
                    Some(options),
                    Ok(tar.unwrap_or_else(Body::empty)),
                );

                self.process_into_stream(req).boxed()
            }
            Err(e) => stream::once(async move { Err(e.into()) }).boxed(),
        }
    }

    /// ---
    ///
    /// # Export Image
    ///
    /// Get a tarball containing all images and metadata for a repository.
    ///
    /// The root of the resulting tar file will contain the file "mainifest.json". If the export is
    /// of an image repository, rather than a signle image, there will also be a `repositories` file
    /// with a JSON description of the exported image repositories.
    /// Additionally, each layer of all exported images will have a sub directory in the archive
    /// containing the filesystem of the layer.
    ///
    /// See the [Docker API documentation](https://docs.docker.com/engine/api/v1.40/#operation/ImageCommit)
    /// for more information.
    /// # Arguments
    /// - The `image_name` string can refer to an individual image and tag (e.g. alpine:latest),
    ///   an individual image by I
    ///
    /// # Returns
    ///  - An uncompressed TAR archive
    pub fn export_image(&self, image_name: &str) -> impl Stream<Item = Result<Bytes, Error>> {
        let url = format!("/images/{}/get", image_name);
        let req = self.build_request(
            &url,
            Builder::new()
                .method(Method::GET)
                .header(CONTENT_TYPE, "application/json"),
            None::<String>,
            Ok(Body::empty()),
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
    ///  Stream.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::ImportImageOptions;
    /// use bollard::errors::Error;
    ///
    /// use std::default::Default;
    /// use futures_util::stream::StreamExt;
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
    ///     let byte_stream = codec::FramedRead::new(file, codec::BytesCodec::new()).map(|r| {
    ///         let bytes = r.unwrap().freeze();
    ///         Ok::<_, Error>(bytes)
    ///     });

    ///     let body = hyper::Body::wrap_stream(byte_stream);

    ///     let mut stream = docker
    ///         .import_image(
    ///             ImportImageOptions {
    ///                 ..Default::default()
    ///             },
    ///             body,
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
        root_fs: Body,
        credentials: Option<HashMap<String, DockerCredentials>>,
    ) -> impl Stream<Item = Result<BuildInfo, Error>> {
        match serde_json::to_string(&credentials.unwrap_or_else(HashMap::new)) {
            Ok(ser_cred) => {
                let req = self.build_request(
                    "/images/load",
                    Builder::new()
                        .method(Method::POST)
                        .header(CONTENT_TYPE, "application/json")
                        .header("X-Registry-Config", base64::encode(&ser_cred)),
                    Some(options),
                    Ok(root_fs),
                );
                self.process_into_stream(req).boxed()
            }
            Err(e) => stream::once(async move { Err(e.into()) }).boxed(),
        }
    }
}
