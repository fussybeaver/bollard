//! Image API: creating, manipulating and pushing docker images
use arrayvec::ArrayVec;
use base64;
use chrono::serde::ts_seconds;
use chrono::{DateTime, Utc};
use futures_core::Stream;
use futures_util::{stream, stream::StreamExt};
use http::header::CONTENT_TYPE;
use http::request::Builder;
use hyper::{body::Bytes, Body, Method};
use serde::Serialize;
use serde_json;

use super::Docker;
use crate::auth::DockerCredentials;
use crate::container::Config;
use crate::docker::{FALSE_STR, TRUE_STR};
use crate::errors::Error;
use crate::errors::ErrorKind::JsonSerializeError;
use crate::models::*;

use std::cmp::Eq;
use std::collections::HashMap;
use std::hash::Hash;

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

/// Subtype for the [Create Image Results](enum.CreateImageResults.html) type.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct CreateImageProgressDetail {
    pub current: Option<u64>,
    pub total: Option<u64>,
}

/// Subtype for the [Create Image Results](enum.CreateImageResults.html) type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateImageErrorDetail {
    message: String,
}

/// Result type for the [Create Image API](../struct.Docker.html#method.create_image)
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            (
                "filters",
                serde_json::to_string(&self.filters)
                    .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
            ),
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
            serde_json::to_string(&self.filters)
                .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
        )]))
    }
}

/// Subtype for the [Prune Image Results](struct.PruneImagesResults.html) type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct PruneImagesImagesDeleted {
    pub untagged: Option<String>,
    pub deleted: Option<String>,
}

/// Result type for the [Prune Images API](../struct.Docker.html#method.prune_images)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct PruneImagesResults {
    pub images_deleted: Option<Vec<PruneImagesImagesDeleted>>,
    pub space_reclaimed: u64,
}

/// Result type for the [Image History API](../struct.Docker.html#method.image_history)
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            (
                "filters",
                serde_json::to_string(&self.filters)
                    .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
            ),
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
            (
                "filters",
                serde_json::to_string(&self.filters)
                    .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
            ),
        ]))
    }
}

/// Result type for the [Image Search API](../struct.Docker.html#method.image_search)
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub changes: Option<T>,
}

/// Trait providing implementations for [Commit Container Options](struct.CommitContainerOptions.html)
/// struct.
#[allow(missing_docs)]
pub trait CommitContainerQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<Vec<(K, V)>, Error>;
}

impl<'a> CommitContainerQueryParams<&'a str, &'a str> for CommitContainerOptions<&'a str> {
    fn into_array(self) -> Result<Vec<(&'a str, &'a str)>, Error> {
        let mut res = vec![
            ("container", self.container),
            ("repo", self.repo),
            ("tag", self.tag),
            ("comment", self.comment),
            ("author", self.author),
            ("pause", if self.pause { TRUE_STR } else { FALSE_STR }),
        ];
        if let Some(c) = self.changes {
            res.push(("changes", c));
        }
        Ok(res)
    }
}

impl<'a> CommitContainerQueryParams<&'a str, String> for CommitContainerOptions<String> {
    fn into_array(self) -> Result<Vec<(&'a str, String)>, Error> {
        let mut res = vec![
            ("container", self.container),
            ("repo", self.repo),
            ("tag", self.tag),
            ("comment", self.comment),
            ("author", self.author),
            ("pause", self.pause.to_string()),
        ];
        if let Some(c) = self.changes {
            res.push(("changes", c));
        }
        Ok(res)
    }
}

/// Result type for the [Commit Container API](../struct.Docker.html#method.commit_container)
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            (
                "cachefrom",
                serde_json::to_string(&self.cachefrom)
                    .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
            ),
            ("pull", self.pull.to_string()),
            ("rm", self.rm.to_string()),
            ("forcerm", self.forcerm.to_string()),
            ("cpusetcpus", self.cpusetcpus.to_string()),
            (
                "buildargs",
                serde_json::to_string(&self.buildargs)
                    .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
            ),
            ("squash", self.squash.to_string()),
            (
                "labels",
                serde_json::to_string(&self.labels)
                    .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
            ),
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
            (
                "cachefrom",
                serde_json::to_string(&self.cachefrom)
                    .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
            ),
            ("pull", self.pull.to_string()),
            ("rm", self.rm.to_string()),
            ("forcerm", self.forcerm.to_string()),
            ("cpusetcpus", self.cpusetcpus.to_string()),
            (
                "buildargs",
                serde_json::to_string(&self.buildargs)
                    .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
            ),
            ("squash", self.squash.to_string()),
            (
                "labels",
                serde_json::to_string(&self.labels)
                    .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
            ),
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

/// Subtype for the [Build Image Results](enum.BuildImageResults.html) type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct BuildImageAuxDetail {
    #[serde(rename = "ID")]
    pub id: String,
}

/// Subtype for the [Build Image Results](enum.BuildImageResults.html) type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct BuildImageErrorDetail {
    pub code: Option<u64>,
    pub message: String,
}

/// Subtype for the [Build Image Results](enum.BuildImageResults.html) type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct BuildImageProgressDetail {
    pub current: Option<u64>,
    pub total: Option<u64>,
}

/// Result type for the [Build Image API](../struct.Docker.html#method.build_image)
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Parameters to the [Import Image API](../struct.Docker.html#method.import_image)
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
#[derive(Debug, Copy, Clone, Default)]
pub struct ImportImageOptions {
    /// Suppress progress details during load.
    pub quiet: bool,
}

/// Trait providing implementations for [Import Image Options](struct.ImportImageOptions.html)
/// struct.
#[allow(missing_docs)]
pub trait ImportImageQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a> ImportImageQueryParams<&'a str, &'a str> for ImportImageOptions {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, &'a str); 1]>, Error> {
        Ok(ArrayVec::from([(
            "quiet",
            if self.quiet { TRUE_STR } else { FALSE_STR },
        )]))
    }
}

/// Result type for the [Import Image API](../struct.Docker.html#method.import_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
#[allow(missing_docs)]
pub enum ImportImageResults {
    #[serde(rename_all = "camelCase")]
    ImportImageStream { stream: String },
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
    ///  - Vector of [API Images](models/struct.ImageSummary.html), wrapped in a Future.
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
    pub async fn list_images<T, K>(&self, options: Option<T>) -> Result<Vec<ImageSummary>, Error>
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
    ///  - An optional [Create Image Options](image/struct.CreateImageOptions.html) struct.
    ///  - An optional request body consisting of a tar or tar.gz archive with the root file system
    ///    for the image. If this argument is used, the value of the `from_src` option must be "-".
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
    /// docker.create_image(options, None, None);
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
        root_fs: Option<Body>,
        credentials: Option<DockerCredentials>,
    ) -> impl Stream<Item = Result<CreateImageResults, Error>>
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
                    match root_fs {
                        Some(body) => Ok(body),
                        None => Ok(Body::empty()),
                    },
                );
                self.process_into_stream(req).boxed()
            }
            Err(e) => {
                stream::once(async move { Err(JsonSerializeError { err: e }.into()) }).boxed()
            }
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
    ///  - [Image](models/struct.Image.html), wrapped in a Future.
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

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
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
    pub async fn prune_images<T, K>(&self, options: Option<T>) -> Result<PruneImagesResults, Error>
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
    ///  - Vector of [Image History Results](image/struct.ImageHistory.html), wrapped in a
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
    pub async fn image_history(&self, image_name: &str) -> Result<Vec<ImageHistory>, Error> {
        let url = format!("/images/{}/history", image_name);

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
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
    ///  - [Search Image Options](image/struct.SearchImagesOptions.html) struct.
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
    pub async fn search_images<T, K>(&self, options: T) -> Result<Vec<APIImageSearch>, Error>
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
    ///  - An optional [Remove Image Options](image/struct.RemoveImageOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [Remove Image Results](image/enum.RemoveImageResults.html), wrapped in a
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
    pub async fn remove_image<T, K, V>(
        &self,
        image_name: &str,
        options: Option<T>,
        credentials: Option<DockerCredentials>,
    ) -> Result<Vec<RemoveImageResults>, Error>
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
                self.process_into_value(req).await
            }
            Err(e) => Err(JsonSerializeError { err: e }.into()),
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
    ///  - Optional [Tag Image Options](image/struct.TagImageOptions.html) struct.
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
    pub async fn tag_image<T, K, V>(
        &self,
        image_name: &str,
        options: Option<T>,
    ) -> Result<(), Error>
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
    ///  - Optional [Push Image Options](image/struct.PushImageOptions.html) struct.
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
    pub async fn push_image<T, K, V>(
        &self,
        image_name: &str,
        options: Option<T>,
        credentials: Option<DockerCredentials>,
    ) -> Result<(), Error>
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

                self.process_into_unit(req).await
            }
            Err(e) => Err(JsonSerializeError { err: e }.into()),
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
    ///  - [Commit Container Results](image/struct.CommitContainerResults.html), wrapped in a Future.
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
    pub async fn commit_container<T, K, V, Z>(
        &self,
        options: T,
        config: Config<Z>,
    ) -> Result<CommitContainerResults, Error>
    where
        T: CommitContainerQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
        Z: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/commit";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            options.into_array().map(|v| Some(v)),
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
    ) -> impl Stream<Item = Result<BuildImageResults, Error>>
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

                self.process_into_stream(req).boxed()
            }
            Err(e) => {
                stream::once(async move { Err(JsonSerializeError { err: e }.into()) }).boxed()
            }
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
        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new()
                .method(Method::GET)
                .header(CONTENT_TYPE, "application/json"),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
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
    ///  - [Image Import Options](./image/struct.ImportImageOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - [Import Image Results](./image/enum.ImportImageResults.html), wrapped in an asynchronous
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
    /// use tokio::fs::File;
    /// use tokio::io::AsyncWriteExt;
    /// use tokio_util::codec;
    /// use tokio::stream::StreamExt;
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
    pub fn import_image<K, V, T>(
        &self,
        options: T,
        root_fs: Body,
        credentials: Option<HashMap<String, DockerCredentials>>,
    ) -> impl Stream<Item = Result<ImportImageResults, Error>>
    where
        T: ImportImageQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        match serde_json::to_string(&credentials.unwrap_or_else(|| HashMap::new())) {
            Ok(ser_cred) => {
                let req = self.build_request(
                    "/images/load",
                    Builder::new()
                        .method(Method::POST)
                        .header(CONTENT_TYPE, "application/json")
                        .header("X-Registry-Config", base64::encode(&ser_cred)),
                    options.into_array().map(|v| Some(v)),
                    Ok(root_fs),
                );
                self.process_into_stream(req).boxed()
            }
            Err(e) => {
                stream::once(async move { Err(JsonSerializeError { err: e }.into()) }).boxed()
            }
        }
    }
}
