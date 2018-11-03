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

/// ## Image
///
/// Image type returned by the [Inspect Image API](../struct.Docker.html#method.inspect_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Metadata {
    pub last_tag_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct RootFS {
    #[serde(rename = "Type")]
    pub type_: String,
    pub layers: Vec<String>,
}

/// ## APIImages
///
/// APIImages type returned by the [List Images API](../struct.Docker.html#method.list_images)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
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

/// ## Create Image Options
///
/// Parameters available for pulling an image, used in the [Create Image
/// API](../struct.Docker.html#method.create_image)
///
/// ## Examples
///
/// ```rust
/// use boondock::image::CreateImageOptions;
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
/// # use boondock::image::CreateImageOptions;
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
    pub from_image: T,
    pub from_src: T,
    pub repo: T,
    pub tag: T,
    pub platform: T,
}

/// ## Create Image Query Params
///
/// Trait providing implementations for [Create Image Options](struct.CreateImageOptions.html)
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

/// ## Create Images Results : Create Image Progress Detail
///
/// Subtype for the [Create Image Results](struct.CreateImagesResults.html) type.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateImageProgressDetail {
    pub current: Option<u64>,
    pub total: Option<u64>,
}

/// ## Create Images Results : Create Image Progress Detail
///
/// Subtype for the [Create Image Results](struct.CreateImagesResults.html) type.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateImageErrorDetail {
    message: String,
}

/// ## Create Image Results
///
/// Result type for the [Create Image API](../struct.Docker.html#method.create_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
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

/// ## List Images Options
///
/// Parameters to the [List Images
/// API](../struct.Docker.html#method.list_images)
///
/// ## Examples
///
/// ```rust
/// use boondock::image::ListImagesOptions;
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
/// # use boondock::image::ListImagesOptions;
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
    pub all: bool,
    pub filters: HashMap<T, T>,
    pub digests: bool,
}

/// ## List Images Query Params
///
/// Trait providing implementations for [List Images Options](struct.ListImagesOptions.html).
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

/// ## Prune Images Options
///
/// Parameters to the [Prune Images API](../struct.Docker.html#method.prune_images)
///
/// ## Examples
///
/// ```rust
/// use boondock::image::PruneImagesOptions;
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
/// # use boondock::image::PruneImagesOptions;
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
    pub filters: HashMap<T, T>,
}

/// ## Prune Images Query Params
///
/// Trait providing implementations for [Prune Images Options](struct.PruneImagesOptions.html).
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

/// ## Prune Images Results : Images Deleted
///
/// Subtype for the [Prune Image Results](struct.PruneImagesResults.html) type.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct PruneImagesImagesDeleted {
    pub untagged: Option<String>,
    pub deleted: Option<String>,
}

/// ## Prune Images Results
///
/// Result type for the [Prune Images API](../struct.Docker.html#method.prune_images)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct PruneImagesResults {
    pub images_deleted: Option<Vec<PruneImagesImagesDeleted>>,
    pub space_reclaimed: u64,
}

/// ## Image History
///
/// Result type for the [Image History API](../struct.Docker.html#method.image_history)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct ImageHistory {
    pub id: String,
    #[serde(with = "ts_seconds")]
    pub created: DateTime<Utc>,
    pub created_by: String,
    pub tags: Option<Vec<String>>,
    pub size: u64,
    pub comment: String,
}

/// ## Search Images Options
///
/// Parameters to the [Search Images API](../struct.Docker.html#method.search_images)
///
/// ## Example
///
/// ```rust
/// use boondock::image::SearchImagesOptions;
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
/// # use boondock::image::SearchImagesOptions;
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
    pub term: T,
    pub limit: Option<u64>,
    pub filters: HashMap<T, T>,
}

/// ## Search Images Query Params
///
/// Trait providing implementations for [Search Images Options](struct.SearchImagesOptions.html)
/// struct.
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

///
/// ## Image Search Results
///
/// Result type for the [Image Search API](../struct.Docker.html#method.image_search)
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct APIImageSearch {
    pub description: String,
    pub is_official: bool,
    pub is_automated: bool,
    pub name: String,
    pub star_count: u64,
}

/// ## Remove Image Options
///
/// Parameters to the [Remove Image API](../struct.Docker.html#method.remove_image)
///
/// ## Examples
///
/// ```rust
/// use boondock::image::RemoveImageOptions;
/// use std::default::Default;
///
/// RemoveImageOptions {
///     force: true,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct RemoveImageOptions {
    pub force: bool,
    pub noprune: bool,
}

/// ## Remove Image Query Params
///
/// Trait providing implementations for [Remove Image Options](struct.RemoveImageOptions.html)
/// struct.
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

/// ## RemoveImageResults
///
/// Result type for the [Remove Image API](../struct.Docker.html#method.remove_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum RemoveImageResults {
    #[serde(rename_all = "PascalCase")]
    RemoveImageUntagged { untagged: String },
    #[serde(rename_all = "PascalCase")]
    RemoveImageDeleted { deleted: String },
}

/// ## Tag Image Options
///
/// Parameters to the [Tag Image API](../struct.Docker.html#method.tag_image)
///
/// ## Examples
///
/// ```rust
/// use boondock::image::TagImageOptions;
/// use std::default::Default;
///
/// let tag_options = TagImageOptions {
///     tag: "v1.0.1",
///     ..Default::default()
/// };
/// ```
///
/// ```rust
/// # use boondock::image::TagImageOptions;
/// # use std::default::Default;
/// let tag_options = TagImageOptions::<String> {
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct TagImageOptions<T> {
    pub repo: T,
    pub tag: T,
}

/// ## Tag Image Query Params
///
/// Trait providing implementations for [Tag Image Options](struct.TagImageOptions.html)
/// struct.
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

/// ## Push Image Options
///
/// Parameters to the [Push Image API](../struct.Docker.html#method.push_image)
///
/// ## Examples
///
/// ```rust
/// use boondock::image::PushImageOptions;
///
/// PushImageOptions {
///     tag: "v1.0.1",
/// };
/// ```
///
/// ```
/// # use boondock::image::PushImageOptions;
/// # use std::default::Default;
/// PushImageOptions::<String> {
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct PushImageOptions<T> {
    pub tag: T,
}

/// ## Push Image Query Params
///
/// Trait providing implementations for [Push Image Options](struct.PushImageOptions.html)
/// struct.
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
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::image::ListImagesOptions;
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

        let req = self.build_request2(
            url,
            Builder::new().method(Method::GET),
            Docker::<C>::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value2(req)
    }

    /// ---
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
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::image::CreateImageOptions;
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

        let req = self.build_request2(
            url,
            Builder::new().method(Method::POST),
            Docker::<C>::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_stream2(req)
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
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use std::default::Default;
    ///
    /// docker.inspect_image("hello-world");
    /// ```
    pub fn inspect_image(&self, image_name: &str) -> impl Future<Item = Image, Error = Error> {
        let url = format!("/images/{}/json", image_name);

        let req = self.build_request2::<_, String, String>(
            &url,
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value2(req)
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
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::image::PruneImagesOptions;
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

        let req = self.build_request2(
            url,
            Builder::new().method(Method::POST),
            Docker::<C>::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value2(req)
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
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.image_history("hello-world");
    /// ```
    pub fn image_history(
        &self,
        image_name: &str,
    ) -> impl Future<Item = Vec<ImageHistory>, Error = Error> {
        let url = format!("/images/{}/history", image_name);

        let req = self.build_request2::<_, String, String>(
            &url,
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value2(req)
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
    /// # use boondock::Docker;
    ///
    /// use boondock::image::SearchImagesOptions;
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

        let req = self.build_request2(
            url,
            Builder::new().method(Method::GET),
            Docker::<C>::transpose_option(Some(options.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value2(req)
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
    /// # use boondock::Docker;
    ///
    /// use boondock::image::RemoveImageOptions;
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

        let req = self.build_request2(
            &url,
            Builder::new().method(Method::DELETE),
            Docker::<C>::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value2(req)
    }

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
    /// # use boondock::Docker;
    ///
    /// use boondock::image::TagImageOptions;
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

        let req = self.build_request2(
            &url,
            Builder::new().method(Method::POST),
            Docker::<C>::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_unit(req)
    }

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
    /// # use boondock::Docker;
    ///
    /// use boondock::auth::DockerCredentials;
    /// use boondock::image::PushImageOptions;
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
                let req = self.build_request2(
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
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::image::CreateImageOptions;
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
    /// # use boondock::Docker;
    ///
    /// use boondock::image::TagImageOptions;
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
    /// # use boondock::Docker;
    ///
    /// use boondock::auth::DockerCredentials;
    /// use boondock::image::PushImageOptions;
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
    /// # use boondock::Docker;
    ///
    /// use boondock::image::RemoveImageOptions;
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
    /// # use boondock::Docker;
    ///
    /// use boondock::image::SearchImagesOptions;
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
    /// # use boondock::Docker;
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
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::image::ListImagesOptions;
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
    /// # use boondock::Docker;
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
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use boondock::image::PruneImagesOptions;
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

#[cfg(test)]
mod tests {

    use super::*;
    use hyper_mock::SequentialConnector;
    use tokio::runtime::Runtime;

    #[test]
    fn list_images() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = SequentialConnector::default();
        connector.content.push(
       "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 310\r\n\r\n[{\"Containers\":-1,\"Created\":1484347856,\"Id\":\"sha256:48b5124b2768d2b917edcb640435044a97967015485e812545546cbed5cf0233\",\"Labels\":{},\"ParentId\":\"\",\"RepoDigests\":[\"hello-world@sha256:c5515758d4c5e1e838e9cd307f6c6a0d620b5e07e6f927b07d05f6d12a1ac8d7\"],\"RepoTags\":null,\"SharedSize\":-1,\"Size\":1840,\"VirtualSize\":1840}]\r\n\r\n".to_string()
     );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let mut filters = HashMap::new();
        filters.insert("dangling", "true");

        let options = Some(ListImagesOptions {
            all: true,
            filters: filters,
            ..Default::default()
        });

        let images = docker.list_images(options);

        let future = images.map(|images| assert_eq!(images[0].size, 1840));

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_create_image() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 542\r\n\r\n{\"status\":\"Pulling from library/hello-world\",\"id\":\"latest\"}\r\n{\"status\":\"Digest: sha256:0add3ace90ecb4adbf7777e9aacf18357296e799f81cabc9fde470971e499788\"}\r\n{\"status\":\"Pulling from library/hello-world\",\"id\":\"linux\"}\r\n{\"status\":\"Digest: sha256:d5c7d767f5ba807f9b363aa4db87d75ab030404a670880e16aedff16f605484b\"}\r\n{\"status\":\"Pulling from library/hello-world\",\"id\":\"nanoserver-1709\"}\r\n{\"errorDetail\":{\"message\":\"no matching manifest for unknown in the manifest list entries\"},\"error\":\"no matching manifest for unknown in the manifest list entries\"}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let options = Some(CreateImageOptions {
            from_image: String::from("hello-world"),
            ..Default::default()
        });

        let stream = docker.create_image(options);

        let future = stream
            .into_future()
            .map(|images| assert_eq!(images.0.is_some(), true));

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e.0);
                Err(e.0)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_inspect_image() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 1744\r\n\r\n{\"Id\":\"sha256:4ab4c602aa5eed5528a6620ff18a1dc4faef0e1ab3a5eddeddb410714478c67f\",\"RepoTags\":[\"hello-world:latest\",\"hello-world:linux\"],\"RepoDigests\":[\"hello-world@sha256:0add3ace90ecb4adbf7777e9aacf18357296e799f81cabc9fde470971e499788\",\"hello-world@sha256:d5c7d767f5ba807f9b363aa4db87d75ab030404a670880e16aedff16f605484b\"],\"Parent\":\"\",\"Comment\":\"\",\"Created\":\"2018-09-07T19:25:39.809797627Z\",\"Container\":\"15c5544a385127276a51553acb81ed24a9429f9f61d6844db1fa34f46348e420\",\"ContainerConfig\":{\"Hostname\":\"15c5544a3851\",\"Domainname\":\"\",\"User\":\"\",\"AttachStdin\":false,\"AttachStdout\":false,\"AttachStderr\":false,\"Tty\":false,\"OpenStdin\":false,\"StdinOnce\":false,\"Env\":[\"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"],\"Cmd\":[\"/bin/sh\",\"-c\",\"#(nop) \",\"CMD [\\\"/hello\\\"]\"],\"ArgsEscaped\":true,\"Image\":\"sha256:9a5813f1116c2426ead0a44bbec252bfc5c3d445402cc1442ce9194fc1397027\",\"Volumes\":null,\"WorkingDir\":\"\",\"Entrypoint\":null,\"OnBuild\":null,\"Labels\":{}},\"DockerVersion\":\"17.06.2-ce\",\"Author\":\"\",\"Config\":{\"Hostname\":\"\",\"Domainname\":\"\",\"User\":\"\",\"AttachStdin\":false,\"AttachStdout\":false,\"AttachStderr\":false,\"Tty\":false,\"OpenStdin\":false,\"StdinOnce\":false,\"Env\":[\"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"],\"Cmd\":[\"/hello\"],\"ArgsEscaped\":true,\"Image\":\"sha256:9a5813f1116c2426ead0a44bbec252bfc5c3d445402cc1442ce9194fc1397027\",\"Volumes\":null,\"WorkingDir\":\"\",\"Entrypoint\":null,\"OnBuild\":null,\"Labels\":null},\"Architecture\":\"amd64\",\"Os\":\"linux\",\"Size\":1840,\"VirtualSize\":1840,\"GraphDriver\":{\"Data\":{\"MergedDir\":\"\",\"UpperDir\":\"\",\"WorkDir\":\"\"},\"Name\":\"overlay2\"},\"RootFS\":{\"Type\":\"layers\",\"Layers\":[\"sha256:428c97da766c4c13b19088a471de6b622b038f3ae8efa10ec5a37d6d31a2df0b\"]},\"Metadata\":{\"LastTagTime\":\"0001-01-01T00:00:00Z\"}}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let image = docker.inspect_image("hello-world");

        let future = image.map(|image| assert_eq!(image.architecture, "amd64"));

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_prune_images() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 42\r\n\r\n{\"ImagesDeleted\":null,\"SpaceReclaimed\":40}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let prune_images_results = docker.prune_images(None::<PruneImagesOptions<String>>);

        let future = prune_images_results.map(|image| assert_eq!(image.space_reclaimed, 40));

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_image_history() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = SequentialConnector::default();
        connector.content.push(
        "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 415\r\n\r\n[{\"Comment\":\"\",\"Created\":1536348339,\"CreatedBy\":\"/bin/sh -c #(nop)  CMD [\\\"/hello\\\"]\",\"Id\":\"sha256:4ab4c602aa5eed5528a6620ff18a1dc4faef0e1ab3a5eddeddb410714478c67f\",\"Size\":0,\"Tags\":[\"hello-world:latest\",\"hello-world:linux\"]},{\"Comment\":\"\",\"Created\":1536348339,\"CreatedBy\":\"/bin/sh -c #(nop) COPY file:9824c33ef192ac944822908370af9f04ab049bfa5c10724e4f727206f5167094 in / \",\"Id\":\"<missing>\",\"Size\":1840,\"Tags\":null}]\r\n\r\n".to_string()
      );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let image_history_results = docker.image_history("hello-world");

        let future = image_history_results.map(|vec| {
            assert!(vec.into_iter().take(1).any(|history| {
                history.tags.unwrap_or(vec![String::new()])[0] == "hello-world:latest"
            }))
        });

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_search_images() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = SequentialConnector::default();
        connector.content.push(
          "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 148\r\n\r\n[{\"star_count\":660,\"is_official\":true,\"name\":\"hello-world\",\"is_automated\":false,\"description\":\"Hello World! (an example of minimal Dockerization)\"}]\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let search_options = SearchImagesOptions {
            term: "hello-world".to_string(),
            ..Default::default()
        };

        let search_results = docker.search_images(search_options);

        let future = search_results.map(|vec| {
            assert!(
                vec.into_iter()
                    .any(|api_image| &api_image.name == "hello-world")
            )
        });

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_remove_image() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = SequentialConnector::default();
        connector.content.push(
          "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 35\r\n\r\n[{\"Untagged\":\"hello-world:latest\"}]\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let remove_options = RemoveImageOptions {
            noprune: true,
            ..Default::default()
        };

        let remove_results = docker.remove_image("hello-world", Some(remove_options));

        let future = remove_results.map(|vec| {
            assert!(vec.into_iter().any(|result| match result {
                RemoveImageResults::RemoveImageUntagged { untagged } => {
                    untagged == "hello-world:latest"
                }
                _ => false,
            }))
        });

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_push_image() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = SequentialConnector::default();
        connector.content.push(
          "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let push_options = PushImageOptions {
            tag: "v1.0.1".to_string(),
        };

        let credentials = DockerCredentials {
            username: Some("Jack".to_string()),
            password: Some("myverysecretpassword".to_string()),
            email: Some("jack.smith@example.com".to_string()),
            serveraddress: Some("localhost:5000".to_string()),
        };

        let results = docker.push_image("hello-world", Some(push_options), Some(credentials));

        let future = results.map(|_| assert!(true));

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_tag_image() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = SequentialConnector::default();
        connector.content.push(
          "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let tag_options = TagImageOptions {
            tag: "v1.0.1".to_string(),
            ..Default::default()
        };

        let results = docker.tag_image("hello-world", Some(tag_options));

        let future = results.map(|_| assert!(true));

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }
}
