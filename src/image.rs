//! Image API: creating, manipulating and pushing docker images
use arrayvec::ArrayVec;
use chrono::serde::ts_seconds;
use chrono::{DateTime, Utc};
use failure::Error;
use futures::future;
use futures::future::Either;
use futures::{stream, Stream};
use http::header::CONTENT_TYPE;
use http::request::Builder;
use hyper::client::connect::Connect;
use hyper::rt::Future;
use hyper::{Body, Method};
use serde::Serialize;
use serde_json;

use super::{Docker, DockerChain};
use auth::DockerCredentials;
use container::{Config, GraphDriver};
use docker::{FALSE_STR, TRUE_STR};
use either::EitherStream;

use std::cmp::Eq;
use std::collections::HashMap;
use std::hash::Hash;

/// Image type returned by the [Inspect Image API](../struct.Docker.html#method.inspect_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
#[allow(missing_docs)]
pub struct Image {
    #[serde(rename = "Id")]
    pub id: String,
    pub container: String,
    pub comment: String,
    pub os: String,
    pub os_version: Option<String>,
    pub architecture: String,
    pub config: Config<String>,
    pub container_config: Config<String>,
    pub parent: String,
    pub created: DateTime<Utc>,
    pub repo_digests: Vec<String>,
    pub repo_tags: Vec<String>,
    #[serde(rename = "RootFS")]
    pub root_fs: RootFS,
    pub size: u64,
    pub docker_version: String,
    pub virtual_size: u64,
    pub author: String,
    pub graph_driver: GraphDriver,
    pub metadata: Metadata,
}

/// Metadata returned by the [Inspect Image API](../struct.Docker.html#method.inspect_image)
#[derive(Debug, Copy, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
#[allow(missing_docs)]
pub struct Metadata {
    pub last_tag_time: DateTime<Utc>,
}

/// Root FS returned by the [Inspect Image API](../struct.Docker.html#method.inspect_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
#[allow(missing_docs)]
pub struct RootFS {
    #[serde(rename = "Type")]
    pub type_: String,
    pub layers: Vec<String>,
}

/// APIImages type returned by the [List Images API](../struct.Docker.html#method.list_images)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
#[allow(missing_docs)]
pub struct APIImages {
    pub id: String,
    pub repo_tags: Option<Vec<String>>,
    #[serde(with = "ts_seconds")]
    pub created: DateTime<Utc>,
    pub size: u64,
    pub virtual_size: u64,
    pub parent_id: String,
    pub repo_digests: Option<Vec<String>>,
    pub labels: Option<HashMap<String, String>>,
    pub containers: isize,
    pub shared_size: i32,
}

/// Parameters available for pulling an image, used in the [Create Image
/// API](../struct.Docker.html#method.create_image)
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
#[derive(Debug, Clone, Default)]
pub struct CreateImageOptions<T>
where
    T: AsRef<str>,
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

/// Trait providing implementations for [Create Image Options](struct.CreateImageOptions.html)
#[allow(missing_docs)]
pub trait CreateImageQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 5]>, Error>;
}

impl<'a> CreateImageQueryParams<&'a str, &'a str> for CreateImageOptions<&'a str> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, &'a str); 5]>, Error> {
        Ok(ArrayVec::from([
            ("fromImage", self.from_image),
            ("fromSrc", self.from_src),
            ("repo", self.repo),
            ("tag", self.tag),
            ("platform", self.platform),
        ]))
    }
}

impl<'a> CreateImageQueryParams<&'a str, String> for CreateImageOptions<String> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 5]>, Error> {
        Ok(ArrayVec::from([
            ("fromImage", self.from_image),
            ("fromSrc", self.from_src),
            ("repo", self.repo),
            ("tag", self.tag),
            ("platform", self.platform),
        ]))
    }
}

/// Subtype for the [Create Image Results](struct.CreateImagesResults.html) type.
#[derive(Debug, Copy, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)]
pub struct CreateImageProgressDetail {
    pub current: Option<u64>,
    pub total: Option<u64>,
}

/// Subtype for the [Create Image Results](struct.CreateImagesResults.html) type.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateImageErrorDetail {
    message: String,
}

/// Result type for the [Create Image API](../struct.Docker.html#method.create_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
#[allow(missing_docs)]
pub enum CreateImageResults {
    #[serde(rename_all = "camelCase")]
    CreateImageProgressResponse {
        status: String,
        progress_detail: Option<CreateImageProgressDetail>,
        id: Option<String>,
        progress: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    CreateImageError {
        error_detail: CreateImageErrorDetail,
        error: String,
    },
}

/// Parameters to the [List Images
/// API](../struct.Docker.html#method.list_images)
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
/// filters.insert("dangling", "true");
///
/// ListImagesOptions{
///   all: true,
///   filters: filters,
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
#[derive(Debug, Clone, Default)]
pub struct ListImagesOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Show all images. Only images from a final layer (no children) are shown by default.
    pub all: bool,
    /// A JSON encoded value of the filters to process on the images list. Available filters:
    ///  - `before`=(`<image-name>[:<tag>]`, `<image id>` or `<image@digest>`)
    ///  - `dangling`=`true`
    ///  - `label`=`key` or `label`=`"key=value"` of an image label
    ///  - `reference`=(`<image-name>[:<tag>]`)
    ///  - `since`=(`<image-name>[:<tag>]`, `<image id>` or `<image@digest>`)
    pub filters: HashMap<T, T>,
    /// Show digest information as a RepoDigests field on each image.
    pub digests: bool,
}

/// Trait providing implementations for [List Images Options](struct.ListImagesOptions.html).
#[allow(missing_docs)]
pub trait ListImagesQueryParams<K>
where
    K: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, String); 3]>, Error>;
}

impl<'a, T: AsRef<str> + Eq + Hash + Serialize> ListImagesQueryParams<&'a str>
    for ListImagesOptions<T>
{
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 3]>, Error> {
        Ok(ArrayVec::from([
            ("all", self.all.to_string()),
            ("filters", serde_json::to_string(&self.filters)?),
            ("digests", self.digests.to_string()),
        ]))
    }
}

/// Parameters to the [Prune Images API](../struct.Docker.html#method.prune_images)
///
/// ## Examples
///
/// ```rust
/// use bollard::image::PruneImagesOptions;
///
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("until", "10m");
///
/// PruneImagesOptions{
///   filters: filters,
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
#[derive(Debug, Clone, Default)]
pub struct PruneImagesOptions<T>
where
    T: AsRef<str> + Eq + Hash,
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
    pub filters: HashMap<T, T>,
}

/// Trait providing implementations for [Prune Images Options](struct.PruneImagesOptions.html).
#[allow(missing_docs)]
pub trait PruneImagesQueryParams<K>
where
    K: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, String); 1]>, Error>;
}

impl<'a, T: AsRef<str> + Eq + Hash + Serialize> PruneImagesQueryParams<&'a str>
    for PruneImagesOptions<T>
{
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 1]>, Error> {
        Ok(ArrayVec::from([(
            "filters",
            serde_json::to_string(&self.filters)?,
        )]))
    }
}

/// Subtype for the [Prune Image Results](struct.PruneImagesResults.html) type.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
#[allow(missing_docs)]
pub struct PruneImagesImagesDeleted {
    pub untagged: Option<String>,
    pub deleted: Option<String>,
}

/// Result type for the [Prune Images API](../struct.Docker.html#method.prune_images)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
#[allow(missing_docs)]
pub struct PruneImagesResults {
    pub images_deleted: Option<Vec<PruneImagesImagesDeleted>>,
    pub space_reclaimed: u64,
}

/// Result type for the [Image History API](../struct.Docker.html#method.image_history)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
#[allow(missing_docs)]
pub struct ImageHistory {
    pub id: String,
    #[serde(with = "ts_seconds")]
    pub created: DateTime<Utc>,
    pub created_by: String,
    pub tags: Option<Vec<String>>,
    pub size: u64,
    pub comment: String,
}

/// Parameters to the [Search Images API](../struct.Docker.html#method.search_images)
///
/// ## Example
///
/// ```rust
/// use bollard::image::SearchImagesOptions;
/// use std::default::Default;
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("until", "10m");
///
/// SearchImagesOptions {
///     term: "hello-world",
///     filters: filters,
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
#[derive(Debug, Clone, Default)]
pub struct SearchImagesOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Term to search (required)
    pub term: T,
    /// Maximum number of results to return
    pub limit: Option<u64>,
    /// A JSON encoded value of the filters to process on the images list. Available filters:
    ///  - `is-automated=(true|false)`
    ///  - `is-official=(true|false)`
    ///  - `stars=<number>` Matches images that has at least 'number' stars.
    pub filters: HashMap<T, T>,
}

/// Trait providing implementations for [Search Images Options](struct.SearchImagesOptions.html)
/// struct.
#[allow(missing_docs)]
pub trait SearchImagesQueryParams<K>
where
    K: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, String); 3]>, Error>;
}

impl<'a> SearchImagesQueryParams<&'a str> for SearchImagesOptions<&'a str> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 3]>, Error> {
        Ok(ArrayVec::from([
            ("term", self.term.to_string()),
            (
                "limit",
                self.limit
                    .map(|limit| limit.to_string())
                    .unwrap_or_else(String::new),
            ),
            ("filters", serde_json::to_string(&self.filters)?),
        ]))
    }
}
impl<'a> SearchImagesQueryParams<&'a str> for SearchImagesOptions<String> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 3]>, Error> {
        Ok(ArrayVec::from([
            ("term", self.term),
            (
                "limit",
                self.limit
                    .map(|limit| limit.to_string())
                    .unwrap_or_else(String::new),
            ),
            ("filters", serde_json::to_string(&self.filters)?),
        ]))
    }
}

/// Result type for the [Image Search API](../struct.Docker.html#method.image_search)
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)]
pub struct APIImageSearch {
    pub description: String,
    pub is_official: bool,
    pub is_automated: bool,
    pub name: String,
    pub star_count: u64,
}

/// Parameters to the [Remove Image API](../struct.Docker.html#method.remove_image)
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
#[derive(Debug, Copy, Clone, Default)]
pub struct RemoveImageOptions {
    /// Remove the image even if it is being used by stopped containers or has other tags.
    pub force: bool,
    /// Do not delete untagged parent images.
    pub noprune: bool,
}

/// Trait providing implementations for [Remove Image Options](struct.RemoveImageOptions.html)
/// struct.
#[allow(missing_docs)]
pub trait RemoveImageQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 2]>, Error>;
}

impl<'a> RemoveImageQueryParams<&'a str, &'a str> for RemoveImageOptions {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, &'a str); 2]>, Error> {
        Ok(ArrayVec::from([
            ("force", if self.force { TRUE_STR } else { FALSE_STR }),
            ("noprune", if self.noprune { TRUE_STR } else { FALSE_STR }),
        ]))
    }
}

/// Result type for the [Remove Image API](../struct.Docker.html#method.remove_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
#[allow(missing_docs)]
pub enum RemoveImageResults {
    #[serde(rename_all = "PascalCase")]
    RemoveImageUntagged { untagged: String },
    #[serde(rename_all = "PascalCase")]
    RemoveImageDeleted { deleted: String },
}

/// Parameters to the [Tag Image API](../struct.Docker.html#method.tag_image)
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
#[derive(Debug, Clone, Default)]
pub struct TagImageOptions<T> {
    /// The repository to tag in. For example, `someuser/someimage`.
    pub repo: T,
    /// The name of the new tag.
    pub tag: T,
}

/// Trait providing implementations for [Tag Image Options](struct.TagImageOptions.html)
/// struct.
#[allow(missing_docs)]
pub trait TagImageQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 2]>, Error>;
}

impl<'a, T: AsRef<str>> TagImageQueryParams<&'a str, T> for TagImageOptions<T> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, T); 2]>, Error> {
        Ok(ArrayVec::from([("repo", self.repo), ("tag", self.tag)]))
    }
}

/// Parameters to the [Push Image API](../struct.Docker.html#method.push_image)
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
#[derive(Debug, Clone, Default)]
pub struct PushImageOptions<T> {
    /// The tag to associate with the image on the registry.
    pub tag: T,
}

/// Trait providing implementations for [Push Image Options](struct.PushImageOptions.html)
/// struct.
#[allow(missing_docs)]
pub trait PushImageQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a, T: AsRef<str>> PushImageQueryParams<&'a str, T> for PushImageOptions<T> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, T); 1]>, Error> {
        Ok(ArrayVec::from([("tag", self.tag)]))
    }
}

impl<C> Docker<C>
where
    C: Connect + Sync + 'static,
{
    /// ---
    ///
    /// # List Images
    ///
    /// Returns a list of images on the server. Note that it uses a different, smaller
    /// representation of an image than inspecting a single image
    ///
    /// # Arguments
    ///
    ///  - An optional [List Images Options](image/struct.ListImagesOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [API Images](image/struct.APIImages.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::ListImagesOptions;
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("dangling", "true");
    ///
    /// let options = Some(ListImagesOptions{
    ///   all: true,
    ///   filters: filters,
    ///   ..Default::default()
    /// });
    ///
    /// docker.list_images(options);
    /// ```
    pub fn list_images<T, K>(
        &self,
        options: Option<T>,
    ) -> impl Future<Item = Vec<APIImages>, Error = Error>
    where
        T: ListImagesQueryParams<K>,
        K: AsRef<str>,
    {
        let url = "/images/json";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            Docker::<C>::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
    }

    /// ---
    ///
    /// # Create Image
    ///
    /// Create an image by either pulling it from a registry or importing it.
    ///
    /// # Arguments
    ///
    ///  - An optional [Create Image Options](image/struct.CreateImageOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - [Create Image Results](image/enum.CreateImageResults.html), wrapped in an asynchronous
    ///  Stream.
    ///
    /// # Examples
    ///
    /// ```rust,norun
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
    /// docker.create_image(options);
    ///
    /// // do some other work while the image is pulled from the docker hub...
    /// ```
    ///
    /// # Unsupported
    ///
    ///  - Import from tarball
    ///
    pub fn create_image<T, K, V>(
        &self,
        options: Option<T>,
    ) -> impl Stream<Item = CreateImageResults, Error = Error>
    where
        T: CreateImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = "/images/create";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            Docker::<C>::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_stream(req)
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
    ///  - [Image](image/struct.Image.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use std::default::Default;
    ///
    /// docker.inspect_image("hello-world");
    /// ```
    pub fn inspect_image(&self, image_name: &str) -> impl Future<Item = Image, Error = Error> {
        let url = format!("/images/{}/json", image_name);

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
    }

    /// ---
    ///
    /// # Prune Images
    ///
    /// Delete unused images.
    ///
    /// # Arguments
    ///
    /// - An optional [Prune Images Options](image/struct.PruneImagesOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - a [Prune Images Results](image/struct.PruneImagesResults.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::PruneImagesOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("until", "10m");
    ///
    /// let options = Some(PruneImagesOptions {
    ///   filters: filters
    /// });
    ///
    /// docker.prune_images(options);
    /// ```
    pub fn prune_images<T, K>(
        &self,
        options: Option<T>,
    ) -> impl Future<Item = PruneImagesResults, Error = Error>
    where
        T: PruneImagesQueryParams<K>,
        K: AsRef<str>,
    {
        let url = "/images/prune";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            Docker::<C>::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
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
    ///  - Vector of [Image History Results](image/struct.ImageHistoryResults.html), wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.image_history("hello-world");
    /// ```
    pub fn image_history(
        &self,
        image_name: &str,
    ) -> impl Future<Item = Vec<ImageHistory>, Error = Error> {
        let url = format!("/images/{}/history", image_name);

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
    }

    /// ---
    ///
    /// # Search Images
    ///
    /// Search for an image on Docker Hub.
    ///
    /// # Arguments
    ///
    ///  - [Search Image Options](struct.SearchImagesOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [API Image Search](image/struct.APIImageSearch.html) results, wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    ///
    /// use bollard::image::SearchImagesOptions;
    /// use std::default::Default;
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("until", "10m");
    ///
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let search_options = SearchImagesOptions {
    ///     term: "hello-world",
    ///     filters: filters,
    ///     ..Default::default()
    /// };
    ///
    /// docker.search_images(search_options);
    /// ```
    pub fn search_images<T, K>(
        &self,
        options: T,
    ) -> impl Future<Item = Vec<APIImageSearch>, Error = Error>
    where
        T: SearchImagesQueryParams<K>,
        K: AsRef<str>,
    {
        let url = "/images/search";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            Docker::<C>::transpose_option(Some(options.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
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
    ///  - An optional [Remove Image Options](image/struct.RemoveImageOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [Remove Image Results](image/struct.RemoveImageResults.html), wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
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
    /// docker.remove_image("hello-world", remove_options);
    /// ```
    pub fn remove_image<T, K, V>(
        &self,
        image_name: &str,
        options: Option<T>,
    ) -> impl Future<Item = Vec<RemoveImageResults>, Error = Error>
    where
        T: RemoveImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/images/{}", image_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            Docker::<C>::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
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
    ///  - Optional [Tag Image Options](struct.TagImageOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
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
    pub fn tag_image<T, K, V>(
        &self,
        image_name: &str,
        options: Option<T>,
    ) -> impl Future<Item = (), Error = Error>
    where
        T: TagImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/images/{}/tag", image_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Docker::<C>::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_unit(req)
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
    ///  - Optional [Push Image Options](struct.PushImageOptions.html) struct.
    ///  - Optional [Docker Credentials](../auth/struct.DockerCredentials.html) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    ///
    /// use bollard::auth::DockerCredentials;
    /// use bollard::image::PushImageOptions;
    ///
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let push_options = Some(PushImageOptions {
    ///     tag: "v1.0.1",
    /// });
    ///
    /// let credentials = Some(DockerCredentials {
    ///     username: Some("Jack".to_string()),
    ///     password: Some("myverysecretpassword".to_string()),
    ///     email: Some("jack.smith@example.com".to_string()),
    ///     serveraddress: Some("localhost:5000".to_string())
    /// });
    ///
    /// docker.push_image("hello-world", push_options, credentials);
    /// ```
    pub fn push_image<T, K, V>(
        &self,
        image_name: &str,
        options: Option<T>,
        credentials: Option<DockerCredentials>,
    ) -> impl Future<Item = (), Error = Error>
    where
        T: PushImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
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
                        .header("X-REGISTRY-AUTH", ser_cred),
                    Docker::<C>::transpose_option(options.map(|o| o.into_array())),
                    Ok(Body::empty()),
                );

                Either::A(self.process_into_unit(req))
            }
            Err(e) => Either::B(future::err(e.into())),
        }
    }
}

impl<C> DockerChain<C>
where
    C: Connect + Sync + 'static,
{
    /// ---
    ///
    /// # Create Image
    ///
    /// Create an image by either pulling it from a registry or importing it. Consumes the client
    /// instance.
    ///
    /// # Arguments
    ///
    ///  - An optional [Create Image Options](image/struct.CreateImageOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a [Create Image
    ///  Results](image/enum.CreateImageResults.html), wrapped in an asynchronous Stream.
    ///
    /// # Examples
    ///
    /// ```rust,norun
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
    /// docker.chain().create_image(options);
    ///
    /// // do some other work while the image is pulled from the docker hub...
    /// ```
    ///
    /// # Unsupported
    ///
    ///  - Import from tarball
    ///
    pub fn create_image<T, K, V>(
        self,
        options: Option<T>,
    ) -> impl Future<
        Item = (
            DockerChain<C>,
            impl Stream<Item = CreateImageResults, Error = Error>,
        ),
        Error = Error,
    >
    where
        T: CreateImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.inner
            .create_image(options)
            .into_future()
            .map(|(first, rest)| match first {
                Some(head) => (self, EitherStream::A(stream::once(Ok(head)).chain(rest))),
                None => (self, EitherStream::B(stream::empty())),
            })
            .map_err(|(err, _)| err)
    }

    /// ---
    ///
    /// # Tag Image
    ///
    /// Tag an image so that it becomes part of a repository. Consumes the instance.
    ///
    /// # Arguments
    ///
    ///  - Image name as a string slice.
    ///  - Optional [Tag Image Options](struct.TagImageOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and the unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    ///
    /// use bollard::image::TagImageOptions;
    ///
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let tag_options = Some(TagImageOptions {
    ///     tag: "v1.0.1",
    ///     ..Default::default()
    /// });
    ///
    /// docker.chain().tag_image("hello-world", tag_options);
    /// ```
    pub fn tag_image<T, K, V>(
        self,
        image_name: &str,
        options: Option<T>,
    ) -> impl Future<Item = (DockerChain<C>, ()), Error = Error>
    where
        T: TagImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.inner
            .tag_image(image_name, options)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Push Image
    ///
    /// Push an image to a registry. Consumes the instance.
    ///
    /// # Arguments
    ///
    ///  - Image name as a string slice.
    ///  - Optional [Push Image Options](struct.PushImageOptions.html) struct.
    ///  - Optional [Docker Credentials](../auth/struct.DockerCredentials.html) struct.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and the unit
    ///  type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    ///
    /// use bollard::auth::DockerCredentials;
    /// use bollard::image::PushImageOptions;
    ///
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let push_options = Some(PushImageOptions {
    ///     tag: "v1.0.1",
    /// });
    ///
    /// let credentials = Some(DockerCredentials {
    ///     username: Some("Jack".to_string()),
    ///     password: Some("myverysecretpassword".to_string()),
    ///     email: Some("jack.smith@example.com".to_string()),
    ///     serveraddress: Some("localhost:5000".to_string())
    /// });
    ///
    /// docker.push_image("hello-world", push_options, credentials);
    /// ```
    pub fn push_image<T, K, V>(
        self,
        image_name: &str,
        options: Option<T>,
        credentials: Option<DockerCredentials>,
    ) -> impl Future<Item = (DockerChain<C>, ()), Error = Error>
    where
        T: PushImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.inner
            .push_image(image_name, options, credentials)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Remove Image
    ///
    /// Remove an image, along with any untagged parent images that were referenced by that image.
    /// Consumes the client instance.
    ///
    /// # Arguments
    ///
    ///  - Image name as a string slice.
    ///  - An optional [Remove Image Options](image/struct.RemoveImageOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a Vector
    /// of [Remove Image Results](image/struct.RemoveImageResults.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    ///
    /// use bollard::image::RemoveImageOptions;
    ///
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let remove_options = Some(RemoveImageOptions {
    ///     force: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.chain().remove_image("hello-world", remove_options);
    /// ```
    pub fn remove_image<T, K, V>(
        self,
        image_name: &str,
        options: Option<T>,
    ) -> impl Future<Item = (DockerChain<C>, Vec<RemoveImageResults>), Error = Error>
    where
        T: RemoveImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.inner
            .remove_image(image_name, options)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Search Images
    ///
    /// Search for an image on Docker Hub. Consumes the client instance.
    ///
    /// # Arguments
    ///
    ///  - [Search Image Options](struct.SearchImagesOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a Vector
    ///  of [API Image Search](image/struct.APIImageSearch.html) results, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    ///
    /// use bollard::image::SearchImagesOptions;
    /// use std::default::Default;
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("until", "10m");
    ///
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let search_options = SearchImagesOptions {
    ///     term: "hello-world",
    ///     filters: filters,
    ///     ..Default::default()
    /// };
    ///
    /// docker.chain().search_images(search_options);
    /// ```
    pub fn search_images<T, K>(
        self,
        options: T,
    ) -> impl Future<Item = (DockerChain<C>, Vec<APIImageSearch>), Error = Error>
    where
        T: SearchImagesQueryParams<K>,
        K: AsRef<str>,
    {
        self.inner
            .search_images(options)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Inspect Image
    ///
    /// Return low-level information about an image. Consumes the client instance.
    ///
    /// # Arguments
    ///
    /// - Image name as a string slice.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and an
    ///  [Image](image/struct.Image.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use std::default::Default;
    ///
    /// docker.chain().inspect_image("hello-world");
    /// ```
    pub fn inspect_image(
        self,
        image_name: &str,
    ) -> impl Future<Item = (DockerChain<C>, Image), Error = Error> {
        self.inner
            .inspect_image(image_name)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # List Images
    ///
    /// Returns a list of images on the server. Note that it uses a different, smaller
    /// representation of an image than inspecting a single image. Consumes the client instance.
    ///
    /// # Arguments
    ///
    ///  - An optional [List Images Options](image/struct.ListImagesOptions.html) struct.
    ///
    ///  # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a Vector
    ///  of [APIImages](image/struct.APIImages.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::ListImagesOptions;
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("dangling", "true");
    ///
    /// let options = Some(ListImagesOptions{
    ///   all: true,
    ///   filters: filters,
    ///   ..Default::default()
    /// });
    ///
    /// docker.chain().list_images(options);
    /// ```
    pub fn list_images<T, K>(
        self,
        options: Option<T>,
    ) -> impl Future<Item = (DockerChain<C>, Vec<APIImages>), Error = Error>
    where
        T: ListImagesQueryParams<K>,
        K: AsRef<str>,
    {
        self.inner.list_images(options).map(|result| (self, result))
    }

    /// ---
    ///
    /// # Image History
    ///
    /// Return parent layers of an image. Consumes the client instance.
    ///
    /// # Arguments
    ///
    ///  - Image name as a string slice.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a Vector
    ///  of [Image History Results](image/struct.ImageHistoryResults.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.chain().image_history("hello-world");
    /// ```
    pub fn image_history(
        self,
        image_name: &str,
    ) -> impl Future<Item = (DockerChain<C>, Vec<ImageHistory>), Error = Error> {
        self.inner
            .image_history(image_name)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Prune Images
    ///
    /// Delete unused images. Consumes the client instance.
    ///
    /// # Arguments
    ///
    /// - An optional [Prune Images Options](image/struct.PruneImagesOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a [Prune Images Results](image/struct.PruneImagesResults.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::image::PruneImagesOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("until", "10m");
    ///
    /// let options = Some(PruneImagesOptions {
    ///   filters: filters
    /// });
    ///
    /// docker.chain().prune_images(options);
    /// ```
    pub fn prune_images<T, K>(
        self,
        options: Option<T>,
    ) -> impl Future<Item = (DockerChain<C>, PruneImagesResults), Error = Error>
    where
        T: PruneImagesQueryParams<K>,
        K: AsRef<str>,
    {
        self.inner
            .prune_images(options)
            .map(|result| (self, result))
    }
}
