//! Image API: creating, manipulating and pushing docker images
use arrayvec::ArrayVec;
use base64;
use chrono::serde::ts_seconds;
use chrono::{DateTime, Utc};
use failure::Error;
use futures::future;
use futures::future::Either;
use futures::{stream, Stream};
use http::header::CONTENT_TYPE;
use http::request::Builder;
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
#[serde(rename_all = "PascalCase")]
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
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct Metadata {
    pub last_tag_time: DateTime<Utc>,
}

/// Root FS returned by the [Inspect Image API](../struct.Docker.html#method.inspect_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct RootFS {
    #[serde(rename = "Type")]
    pub type_: String,
    pub layers: Vec<String>,
}

/// APIImages type returned by the [List Images API](../struct.Docker.html#method.list_images)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
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
#[allow(missing_docs)]
pub struct CreateImageProgressDetail {
    pub current: Option<u64>,
    pub total: Option<u64>,
}

/// Subtype for the [Create Image Results](struct.CreateImagesResults.html) type.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateImageErrorDetail {
    message: String,
}

/// Result type for the [Create Image API](../struct.Docker.html#method.create_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
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
/// filters.insert("dangling", vec!["true"]);
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
    pub filters: HashMap<T, Vec<T>>,
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
/// filters.insert("until", vec!["10m"]);
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
    pub filters: HashMap<T, Vec<T>>,
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
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct PruneImagesImagesDeleted {
    pub untagged: Option<String>,
    pub deleted: Option<String>,
}

/// Result type for the [Prune Images API](../struct.Docker.html#method.prune_images)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct PruneImagesResults {
    pub images_deleted: Option<Vec<PruneImagesImagesDeleted>>,
    pub space_reclaimed: u64,
}

/// Result type for the [Image History API](../struct.Docker.html#method.image_history)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
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
#[serde(untagged)]
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

/// Parameters to the [Commit Container API](../struct.Docker.html#method.commit_container)
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
#[derive(Debug, Clone, Default)]
pub struct CommitContainerOptions<T> {
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
    pub changes: T,
}

/// Trait providing implementations for [Commit Container Options](struct.CommitContainerOptions.html)
/// struct.
#[allow(missing_docs)]
pub trait CommitContainerQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 7]>, Error>;
}

impl<'a> CommitContainerQueryParams<&'a str, &'a str> for CommitContainerOptions<&'a str> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, &'a str); 7]>, Error> {
        Ok(ArrayVec::from([
            ("container", self.container),
            ("repo", self.repo),
            ("tag", self.tag),
            ("comment", self.comment),
            ("author", self.author),
            ("pause", if self.pause { TRUE_STR } else { FALSE_STR }),
            ("changes", self.changes),
        ]))
    }
}

impl<'a> CommitContainerQueryParams<&'a str, String> for CommitContainerOptions<String> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 7]>, Error> {
        Ok(ArrayVec::from([
            ("container", self.container),
            ("repo", self.repo),
            ("tag", self.tag),
            ("comment", self.comment),
            ("author", self.author),
            ("pause", self.pause.to_string()),
            ("changes", self.changes),
        ]))
    }
}

/// Result type for the [Commit Container API](../struct.Docker.html#method.commit_container)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct CommitContainerResults {
    pub id: String,
}

/// Parameters to the [Build Image API](../struct.Docker.html#method.build_image)
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
#[derive(Debug, Clone, Default)]
pub struct BuildImageOptions<T>
where
    T: AsRef<str> + Eq + Hash,
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
    pub buildargs: HashMap<T, T>,
    /// Size of `/dev/shm` in bytes. The size must be greater than 0. If omitted the system uses 64MB.
    pub shmsize: Option<u64>,
    /// Squash the resulting images layers into a single layer.
    pub squash: bool,
    /// Arbitrary key/value labels to set on the image, as a JSON map of string pairs.
    pub labels: HashMap<T, T>,
    /// Sets the networking mode for the run commands during build. Supported standard values are:
    /// `bridge`, `host`, `none`, and `container:<name|id>`. Any other value is taken as a custom network's
    /// name to which this container should connect to.
    pub networkmode: T,
    /// Platform in the format `os[/arch[/variant]]`
    pub platform: T,
}

/// Trait providing implementations for [Build Image Options](struct.BuildImageOptions.html)
/// struct.
#[allow(missing_docs)]
pub trait BuildImageQueryParams<K>
where
    K: AsRef<str>,
{
    fn into_array(self) -> Result<Vec<(K, String)>, Error>;
}

impl<'a> BuildImageQueryParams<&'a str> for BuildImageOptions<&'a str> {
    fn into_array(self) -> Result<Vec<(&'a str, String)>, Error> {
        let mut output = vec![
            ("dockerfile", self.dockerfile.to_string()),
            ("t", self.t.to_string()),
            ("remote", self.remote.to_string()),
            ("q", self.q.to_string()),
            ("nocache", self.nocache.to_string()),
            ("cachefrom", serde_json::to_string(&self.cachefrom)?),
            ("pull", self.pull.to_string()),
            ("rm", self.rm.to_string()),
            ("forcerm", self.forcerm.to_string()),
            ("cpusetcpus", self.cpusetcpus.to_string()),
            ("buildargs", serde_json::to_string(&self.buildargs)?),
            ("squash", self.squash.to_string()),
            ("labels", serde_json::to_string(&self.labels)?),
            ("networkmode", self.networkmode.to_string()),
            ("platform", self.platform.to_string()),
        ];

        output.extend(
            vec![
                self.extrahosts.map(|v| ("extrahosts", v.to_string())),
                self.memory.map(|v| ("memory", v.to_string())),
                self.cpushares.map(|v| ("cpushares", v.to_string())),
                self.cpuperiod.map(|v| ("cpuperiod", v.to_string())),
                self.cpuquota.map(|v| ("cpuperiod", v.to_string())),
                self.shmsize.map(|v| ("shmsize", v.to_string())),
            ]
            .into_iter()
            .flatten(),
        );

        Ok(output)
    }
}

impl<'a> BuildImageQueryParams<&'a str> for BuildImageOptions<String> {
    fn into_array(self) -> Result<Vec<(&'a str, String)>, Error> {
        let mut output = vec![
            ("dockerfile", self.dockerfile),
            ("t", self.t),
            ("remote", self.remote),
            ("q", self.q.to_string()),
            ("nocache", self.nocache.to_string()),
            ("cachefrom", serde_json::to_string(&self.cachefrom)?),
            ("pull", self.pull.to_string()),
            ("rm", self.rm.to_string()),
            ("forcerm", self.forcerm.to_string()),
            ("cpusetcpus", self.cpusetcpus.to_string()),
            ("buildargs", serde_json::to_string(&self.buildargs)?),
            ("squash", self.squash.to_string()),
            ("labels", serde_json::to_string(&self.labels)?),
            ("networkmode", self.networkmode),
            ("platform", self.platform),
        ];

        output.extend(
            vec![
                self.extrahosts.map(|v| ("extrahosts", v)),
                self.memory.map(|v| ("memory", v.to_string())),
                self.cpushares.map(|v| ("cpushares", v.to_string())),
                self.cpuperiod.map(|v| ("cpuperiod", v.to_string())),
                self.cpuquota.map(|v| ("cpuperiod", v.to_string())),
                self.shmsize.map(|v| ("shmsize", v.to_string())),
            ]
            .into_iter()
            .flatten(),
        );

        Ok(output)
    }
}

/// Subtype for the [Build Image Results](struct.BuildImageResults.html) type.
#[derive(Debug, Clone, Deserialize)]
#[allow(missing_docs)]
pub struct BuildImageAuxDetail {
    #[serde(rename = "ID")]
    pub id: String,
}

/// Subtype for the [Build Image Results](struct.BuildImageResults.html) type.
#[derive(Debug, Clone, Deserialize)]
#[allow(missing_docs)]
pub struct BuildImageErrorDetail {
    pub code: Option<u64>,
    pub message: String,
}

/// Subtype for the [Build Image Results](struct.BuildImageResults.html) type.
#[derive(Debug, Clone, Copy, Deserialize)]
#[allow(missing_docs)]
pub struct BuildImageProgressDetail {
    pub current: Option<u64>,
    pub total: Option<u64>,
}

/// Result type for the [Build Image API](../struct.Docker.html#method.build_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
#[allow(missing_docs)]
pub enum BuildImageResults {
    BuildImageStream {
        stream: String,
    },
    BuildImageAux {
        aux: BuildImageAuxDetail,
    },
    #[serde(rename_all = "camelCase")]
    BuildImageError {
        error_detail: BuildImageErrorDetail,
        error: String,
    },
    #[serde(rename_all = "camelCase")]
    BuildImageStatus {
        status: String,
        progress_detail: Option<BuildImageProgressDetail>,
        progress: Option<String>,
        id: Option<String>,
    },
    BuildImageNone {},
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
    ///  - An optional [List Images Options](image/struct.ListImagesOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [API Images](image/struct.APIImages.html), wrapped in a Future.
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
            Docker::transpose_option(options.map(|o| o.into_array())),
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
    /// docker.create_image(options, None);
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
        credentials: Option<DockerCredentials>,
    ) -> impl Stream<Item = CreateImageResults, Error = Error>
    where
        T: CreateImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
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
                    Docker::transpose_option(options.map(|o| o.into_array())),
                    Ok(Body::empty()),
                );
                EitherStream::A(self.process_into_stream(req))
            }
            Err(e) => EitherStream::B(future::err(e.into()).into_stream()),
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
    ///  - [Image](image/struct.Image.html), wrapped in a Future.
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
            Docker::transpose_option(options.map(|o| o.into_array())),
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
    /// ```rust
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
    /// ```rust
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
            Docker::transpose_option(Some(options.into_array())),
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
    pub fn remove_image<T, K, V>(
        &self,
        image_name: &str,
        options: Option<T>,
        credentials: Option<DockerCredentials>,
    ) -> impl Future<Item = Vec<RemoveImageResults>, Error = Error>
    where
        T: RemoveImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
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
                    Docker::transpose_option(options.map(|o| o.into_array())),
                    Ok(Body::empty()),
                );
                Either::A(self.process_into_value(req))
            }
            Err(e) => Either::B(future::err(e.into())),
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
    ///  - Optional [Tag Image Options](struct.TagImageOptions.html) struct.
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
            Docker::transpose_option(options.map(|o| o.into_array())),
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
    ///  - Optional [Docker Credentials](auth/struct.DockerCredentials.html) struct.
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
                        .header("X-Registry-Auth", base64::encode(&ser_cred)),
                    Docker::transpose_option(options.map(|o| o.into_array())),
                    Ok(Body::empty()),
                );

                Either::A(self.process_into_unit(req))
            }
            Err(e) => Either::B(future::err(e.into())),
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
    ///  - [Commit Container Options](image/struct.CommitContainerOptions.html) struct.
    ///  - Container [Config](container/struct.Config.html) struct.
    ///
    /// # Returns
    ///
    ///  - [Commit Container Results](container/struct.CommitContainerResults.html), wrapped in a Future.
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
    pub fn commit_container<T, K, V, Z>(
        &self,
        options: T,
        config: Config<Z>,
    ) -> impl Future<Item = CommitContainerResults, Error = Error>
    where
        T: CommitContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
        Z: AsRef<str> + Eq + Hash + Serialize,
    {
        let url = "/commit";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            options.into_array().map(|v| Some(v)),
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_value(req)
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
    ///  - [Build Image Options](image/struct.BuildImageOptions.html) struct.
    ///  - Optional [Docker Credentials](auth/struct.DockerCredentials.html) struct.
    ///  - Tar archive compressed with one of the following algorithms: identity (no compression),
    ///    gzip, bzip2, xz. Optional [Hyper Body](https://hyper.rs/hyper/master/hyper/struct.Body.html).
    ///
    /// # Returns
    ///
    ///  - [Build Image Results](image/enum.BuildImageResults.html), wrapped in an asynchronous
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
    pub fn build_image<T, K>(
        &self,
        options: T,
        credentials: Option<HashMap<String, DockerCredentials>>,
        tar: Option<Body>,
    ) -> impl Stream<Item = BuildImageResults, Error = Error>
    where
        T: BuildImageQueryParams<K>,
        K: AsRef<str>,
    {
        let url = "/build";

        match serde_json::to_string(&credentials.unwrap_or_else(|| HashMap::new())) {
            Ok(ser_cred) => {
                let req = self.build_request(
                    &url,
                    Builder::new()
                        .method(Method::POST)
                        .header(CONTENT_TYPE, "application/x-tar")
                        .header("X-Registry-Config", base64::encode(&ser_cred)),
                    options.into_array().map(|v| Some(v)),
                    Ok(tar.unwrap_or_else(|| Body::empty())),
                );

                EitherStream::A(self.process_into_stream(req))
            }
            Err(e) => EitherStream::B(future::err(e.into()).into_stream()),
        }
    }
}

impl DockerChain {
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
    /// docker.chain().create_image(options, None);
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
        credentials: Option<DockerCredentials>,
    ) -> impl Future<
        Item = (
            DockerChain,
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
            .create_image(options, credentials)
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
    /// ```rust
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
    ) -> impl Future<Item = (DockerChain, ()), Error = Error>
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
    pub fn push_image<T, K, V>(
        self,
        image_name: &str,
        options: Option<T>,
        credentials: Option<DockerCredentials>,
    ) -> impl Future<Item = (DockerChain, ()), Error = Error>
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
    /// ```rust
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
    /// docker.chain().remove_image("hello-world", remove_options, None);
    /// ```
    pub fn remove_image<T, K, V>(
        self,
        image_name: &str,
        options: Option<T>,
        credentials: Option<DockerCredentials>,
    ) -> impl Future<Item = (DockerChain, Vec<RemoveImageResults>), Error = Error>
    where
        T: RemoveImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.inner
            .remove_image(image_name, options, credentials)
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
    /// ```rust
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
    ) -> impl Future<Item = (DockerChain, Vec<APIImageSearch>), Error = Error>
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
    /// ```rust
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
    ) -> impl Future<Item = (DockerChain, Image), Error = Error> {
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
    /// ```rust
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
    ///   filters: filters,
    ///   ..Default::default()
    /// });
    ///
    /// docker.chain().list_images(options);
    /// ```
    pub fn list_images<T, K>(
        self,
        options: Option<T>,
    ) -> impl Future<Item = (DockerChain, Vec<APIImages>), Error = Error>
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
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.chain().image_history("hello-world");
    /// ```
    pub fn image_history(
        self,
        image_name: &str,
    ) -> impl Future<Item = (DockerChain, Vec<ImageHistory>), Error = Error> {
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
    ///   filters: filters
    /// });
    ///
    /// docker.chain().prune_images(options);
    /// ```
    pub fn prune_images<T, K>(
        self,
        options: Option<T>,
    ) -> impl Future<Item = (DockerChain, PruneImagesResults), Error = Error>
    where
        T: PruneImagesQueryParams<K>,
        K: AsRef<str>,
    {
        self.inner
            .prune_images(options)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Commit Container
    ///
    /// Create a new image from a container.
    ///
    /// # Arguments
    ///
    ///  - [Commit Container Options](image/struct.CommitContainerOptions.html) struct.
    ///  - Container [Config](container/struct.Config.html) struct.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a [Commit Container Results](container/struct.CommitContainerResults.html), wrapped in a Future.
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
    /// docker.chain().commit_container(options, config);
    /// ```
    pub fn commit_container<T, K, V, Z>(
        self,
        options: T,
        config: Config<Z>,
    ) -> impl Future<Item = (DockerChain, CommitContainerResults), Error = Error>
    where
        T: CommitContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
        Z: AsRef<str> + Eq + Hash + Serialize,
    {
        self.inner
            .commit_container(options, config)
            .map(|result| (self, result))
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
    ///  - [Build Image Options](image/struct.BuildImageOptions.html) struct.
    ///  - Optional [Docker Credentials](auth/struct.DockerCredentials.html) struct.
    ///  - Tar archive compressed with one of the following algorithms: identity (no compression),
    ///    gzip, bzip2, xz. Optional [Hyper Body](https://hyper.rs/hyper/master/hyper/struct.Body.html).
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a [Build
    ///  Image Results](image/enum.BuildImageResults.html), wrapped in an asynchronous Stream.
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
    pub fn build_image<T, K>(
        self,
        options: T,
        credentials: Option<HashMap<String, DockerCredentials>>,
        tar: Option<Body>,
    ) -> impl Future<
        Item = (
            DockerChain,
            impl Stream<Item = BuildImageResults, Error = Error>,
        ),
        Error = Error,
    >
    where
        T: BuildImageQueryParams<K>,
        K: AsRef<str>,
    {
        self.inner
            .build_image(options, credentials, tar)
            .into_future()
            .map(|(first, rest)| match first {
                Some(head) => (self, EitherStream::A(stream::once(Ok(head)).chain(rest))),
                None => (self, EitherStream::B(stream::empty())),
            })
            .map_err(|(err, _)| err)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use hyper_mock::HostToReplyConnector;
    use tokio::runtime::Runtime;

    #[test]
    fn list_images() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 310\r\n\r\n[{\"Containers\":-1,\"Created\":1484347856,\"Id\":\"sha256:48b5124b2768d2b917edcb640435044a97967015485e812545546cbed5cf0233\",\"Labels\":{},\"ParentId\":\"\",\"RepoDigests\":[\"hello-world@sha256:c5515758d4c5e1e838e9cd307f6c6a0d620b5e07e6f927b07d05f6d12a1ac8d7\"],\"RepoTags\":null,\"SharedSize\":-1,\"Size\":1840,\"VirtualSize\":1840}]\r\n\r\n".to_string()
     );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

        let mut filters = HashMap::new();
        filters.insert("dangling", vec!["true"]);
        filters.insert("label", vec!["maintainer=some_maintainer"]);

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
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 542\r\n\r\n{\"status\":\"Pulling from library/hello-world\",\"id\":\"latest\"}\r\n{\"status\":\"Digest: sha256:0add3ace90ecb4adbf7777e9aacf18357296e799f81cabc9fde470971e499788\"}\r\n{\"status\":\"Pulling from library/hello-world\",\"id\":\"linux\"}\r\n{\"status\":\"Digest: sha256:d5c7d767f5ba807f9b363aa4db87d75ab030404a670880e16aedff16f605484b\"}\r\n{\"status\":\"Pulling from library/hello-world\",\"id\":\"nanoserver-1709\"}\r\n{\"errorDetail\":{\"message\":\"no matching manifest for unknown in the manifest list entries\"},\"error\":\"no matching manifest for unknown in the manifest list entries\"}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

        let options = Some(CreateImageOptions {
            from_image: String::from("hello-world"),
            ..Default::default()
        });

        let stream = docker.create_image(options, None);

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
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 1744\r\n\r\n{\"Id\":\"sha256:4ab4c602aa5eed5528a6620ff18a1dc4faef0e1ab3a5eddeddb410714478c67f\",\"RepoTags\":[\"hello-world:latest\",\"hello-world:linux\"],\"RepoDigests\":[\"hello-world@sha256:0add3ace90ecb4adbf7777e9aacf18357296e799f81cabc9fde470971e499788\",\"hello-world@sha256:d5c7d767f5ba807f9b363aa4db87d75ab030404a670880e16aedff16f605484b\"],\"Parent\":\"\",\"Comment\":\"\",\"Created\":\"2018-09-07T19:25:39.809797627Z\",\"Container\":\"15c5544a385127276a51553acb81ed24a9429f9f61d6844db1fa34f46348e420\",\"ContainerConfig\":{\"Hostname\":\"15c5544a3851\",\"Domainname\":\"\",\"User\":\"\",\"AttachStdin\":false,\"AttachStdout\":false,\"AttachStderr\":false,\"Tty\":false,\"OpenStdin\":false,\"StdinOnce\":false,\"Env\":[\"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"],\"Cmd\":[\"/bin/sh\",\"-c\",\"#(nop) \",\"CMD [\\\"/hello\\\"]\"],\"ArgsEscaped\":true,\"Image\":\"sha256:9a5813f1116c2426ead0a44bbec252bfc5c3d445402cc1442ce9194fc1397027\",\"Volumes\":null,\"WorkingDir\":\"\",\"Entrypoint\":null,\"OnBuild\":null,\"Labels\":{}},\"DockerVersion\":\"17.06.2-ce\",\"Author\":\"\",\"Config\":{\"Hostname\":\"\",\"Domainname\":\"\",\"User\":\"\",\"AttachStdin\":false,\"AttachStdout\":false,\"AttachStderr\":false,\"Tty\":false,\"OpenStdin\":false,\"StdinOnce\":false,\"Env\":[\"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"],\"Cmd\":[\"/hello\"],\"ArgsEscaped\":true,\"Image\":\"sha256:9a5813f1116c2426ead0a44bbec252bfc5c3d445402cc1442ce9194fc1397027\",\"Volumes\":null,\"WorkingDir\":\"\",\"Entrypoint\":null,\"OnBuild\":null,\"Labels\":null},\"Architecture\":\"amd64\",\"Os\":\"linux\",\"Size\":1840,\"VirtualSize\":1840,\"GraphDriver\":{\"Data\":{\"MergedDir\":\"\",\"UpperDir\":\"\",\"WorkDir\":\"\"},\"Name\":\"overlay2\"},\"RootFS\":{\"Type\":\"layers\",\"Layers\":[\"sha256:428c97da766c4c13b19088a471de6b622b038f3ae8efa10ec5a37d6d31a2df0b\"]},\"Metadata\":{\"LastTagTime\":\"0001-01-01T00:00:00Z\"}}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

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
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 42\r\n\r\n{\"ImagesDeleted\":null,\"SpaceReclaimed\":40}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

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
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 415\r\n\r\n[{\"Comment\":\"\",\"Created\":1536348339,\"CreatedBy\":\"/bin/sh -c #(nop)  CMD [\\\"/hello\\\"]\",\"Id\":\"sha256:4ab4c602aa5eed5528a6620ff18a1dc4faef0e1ab3a5eddeddb410714478c67f\",\"Size\":0,\"Tags\":[\"hello-world:latest\",\"hello-world:linux\"]},{\"Comment\":\"\",\"Created\":1536348339,\"CreatedBy\":\"/bin/sh -c #(nop) COPY file:9824c33ef192ac944822908370af9f04ab049bfa5c10724e4f727206f5167094 in / \",\"Id\":\"<missing>\",\"Size\":1840,\"Tags\":null}]\r\n\r\n".to_string()
      );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

        let image_history_results = docker.image_history("hello-world");

        let future = image_history_results.map(|vec| {
            assert!(vec
                .into_iter()
                .take(1)
                .any(|history| history.tags.unwrap_or(vec![String::new()])[0]
                    == "hello-world:latest"))
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
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 148\r\n\r\n[{\"star_count\":660,\"is_official\":true,\"name\":\"hello-world\",\"is_automated\":false,\"description\":\"Hello World! (an example of minimal Dockerization)\"}]\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

        let search_options = SearchImagesOptions {
            term: "hello-world".to_string(),
            ..Default::default()
        };

        let search_results = docker.search_images(search_options);

        let future = search_results.map(|vec| {
            assert!(vec
                .into_iter()
                .any(|api_image| &api_image.name == "hello-world"))
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
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 35\r\n\r\n[{\"Untagged\":\"hello-world:latest\"}]\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

        let remove_options = RemoveImageOptions {
            noprune: true,
            ..Default::default()
        };

        let remove_results = docker.remove_image("hello-world", Some(remove_options), None);

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
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

        let push_options = PushImageOptions {
            tag: "v1.0.1".to_string(),
        };

        let credentials = DockerCredentials {
            username: Some("Jack".to_string()),
            password: Some("myverysecretpassword".to_string()),
            ..Default::default()
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
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

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

    #[test]
    fn test_commit_container() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 80\r\n\r\n{\"Id\":\"sha256:c69d56ed58eb9b519bb3de569de7e83f5c3eff57858eaa7883a9e206cf7ca5eb\"}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

        let commit_container_options = CommitContainerOptions {
            container: "my-running-container",
            pause: true,
            ..Default::default()
        };

        let config = Config::<String> {
            ..Default::default()
        };

        let results = docker.commit_container(commit_container_options, config);

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
    fn test_build_image() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),

            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/x-tar\r\nContent-Length: 520\r\n\r\n{\"stream\":\"Step 1/2 : FROM alpine\"}\r\n{\"stream\":\"\\n\"}\r\n{\"stream\":\" ---\\u003e 3f53bb00af94\\n\"}\r\n{\"stream\":\"Step 2/2 : RUN touch bollard.txt\"}\r\n{\"stream\":\"\\n\"}\r\n{\"stream\":\" ---\\u003e Running in 853fceb48e80\\n\"}\r\n{\"stream\":\"Removing intermediate container 853fceb48e80\\n\"}\r\n{\"stream\":\" ---\\u003e 5949ad5433c9\\n\"}\r\n{\"aux\":{\"ID\":\"sha256:5949ad5433c96bb38c6a60acc84653600ccb06f1bdd7216acdba752bc2da7460\"}}\r\n{\"stream\":\"Successfully built 5949ad5433c9\\n\"}\r\n{\"stream\":\"Successfully tagged integration_test_build_image:latest\\n\"}\r\n".to_string()
        );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

        let build_image_options = BuildImageOptions {
            t: "my-image",
            rm: true,
            ..Default::default()
        };

        let mut credentials = HashMap::new();
        credentials.insert(
            "quay.io".to_string(),
            DockerCredentials {
                username: Some("Jack".to_string()),
                password: Some("myverysecretpassword".to_string()),
                ..Default::default()
            },
        );

        let results = docker.build_image(
            build_image_options,
            Some(credentials),
            Some(Vec::new().into()),
        );

        let future = results.collect().map(|vec| {
            assert!(vec.into_iter().any(|result| match result {
                BuildImageResults::BuildImageStream { stream } => {
                    stream == "Successfully tagged integration_test_build_image:latest\n"
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
}
