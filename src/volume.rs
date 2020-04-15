//! Volume API: Create and manage persistent storage that can be attached to containers.

use arrayvec::ArrayVec;
use chrono::{DateTime, Utc};
use http::request::Builder;
use hyper::{Body, Method};
use serde::Serialize;
use serde_json;

use std::cmp::Eq;
use std::collections::HashMap;
use std::hash::Hash;

use super::Docker;
use crate::docker::{FALSE_STR, TRUE_STR};
use crate::errors::Error;
use crate::errors::ErrorKind::JsonSerializeError;

/// Subresult type for the [List Volumes API](../struct.Docker.html#method.list_volumes)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct VolumesListVolumesResults {
    pub created_at: DateTime<Utc>,
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub labels: Option<HashMap<String, String>>,
    pub scope: String,
    pub options: Option<HashMap<String, String>>,
}

/// Result type for the [List Volumes API](../struct.Docker.html#method.list_volumes)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ListVolumesResults {
    pub volumes: Vec<VolumesListVolumesResults>,
    pub warnings: Option<Vec<String>>,
}

/// Parameters used in the [List Volume API](../struct.Docker.html#method.list_volumes)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListVolumesOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// JSON encoded value of the filters (a `map[string][]string`) to process on the volumes list. Available filters:
    ///  - `dangling=<boolean>` When set to `true` (or `1`), returns all volumes that are not in use by a container. When set to `false` (or `0`), only volumes that are in use by one or more containers are returned.
    ///  - `driver=<volume-driver-name>` Matches volumes based on their driver.
    ///  - `label=<key>` or `label=<key>:<value>` Matches volumes based on the presence of a `label` alone or a `label` and a value.
    ///  - `name=<volume-name>` Matches all or part of a volume name.
    pub filters: HashMap<T, Vec<T>>,
}

#[allow(missing_docs)]
/// Trait providing implementations for [List Volumes Options](struct.ListVolumesOptions.html)
/// struct.
pub trait ListVolumesQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, String); 1]>, Error>;
}

impl<'a, T: AsRef<str> + Eq + Hash + Serialize> ListVolumesQueryParams<&'a str, String>
    for ListVolumesOptions<T>
{
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 1]>, Error> {
        Ok(ArrayVec::from([(
            "filters",
            serde_json::to_string(&self.filters)
                .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
        )]))
    }
}

/// Result type for the [Inspect Volume API](../struct.Docker.html#method.inspect_volume) and the
/// [Create Volume API](../struct.Docker.html#method.create_volume)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct VolumeAPI {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub labels: HashMap<String, String>,
    pub scope: String,
    pub created_at: DateTime<Utc>,
}

/// Volume configuration used in the [Create Volume
/// API](../struct.Docker.html#method.create_volume)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateVolumeOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// The new volume's name. If not specified, Docker generates a name.
    pub name: T,
    /// Name of the volume driver to use.
    pub driver: T,
    /// A mapping of driver options and values. These options are passed directly to the driver and
    /// are driver specific.
    pub driver_opts: HashMap<T, T>,
    /// User-defined key/value metadata.
    pub labels: HashMap<T, T>,
}

/// Parameters used in the [Remove Volume API](../struct.Docker.html#method.remove_volume)
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveVolumeOptions {
    /// Force the removal of the volume.
    pub force: bool,
}

#[allow(missing_docs)]
/// Trait providing implementations for [Remove Volume Options](struct.RemoveVolumeOptions.html)
/// struct.
pub trait RemoveVolumeQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a> RemoveVolumeQueryParams<&'a str, &'a str> for RemoveVolumeOptions {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, &'a str); 1]>, Error> {
        Ok(ArrayVec::from([(
            "force",
            if self.force { TRUE_STR } else { FALSE_STR },
        )]))
    }
}

/// Parameters used in the [Prune Volumes API](../struct.Docker.html#method.prune_volumes)
///
/// ## Examples
///
/// ```rust
/// use bollard::volume::PruneVolumesOptions;
///
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("label!", vec!("maintainer=some_maintainer"));
///
/// PruneVolumesOptions{
///     filters: filters
/// };
/// ```
///
/// ```rust
/// # use bollard::volume::PruneVolumesOptions;
/// # use std::default::Default;
///
/// PruneVolumesOptions::<&str>{
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct PruneVolumesOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Filters to process on the prune list, encoded as JSON.
    ///  - `label` (`label=<key>`, `label=<key>=<value>`, `label!=<key>`, or
    ///  `label!=<key>=<value>`) Prune volumes with (or without, in case `label!=...` is used) the
    ///  specified labels.
    pub filters: HashMap<T, Vec<T>>,
}

/// Trait providing implementations for [Prune Volumes Options](struct.PruneVolumesOptions.html)
/// struct.
#[allow(missing_docs)]
pub trait PruneVolumesQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, String); 1]>, Error>;
}

impl<'a> PruneVolumesQueryParams<&'a str, String> for PruneVolumesOptions<&'a str> {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 1]>, Error> {
        Ok(ArrayVec::from([(
            "filters",
            serde_json::to_string(&self.filters)
                .map_err::<Error, _>(|e| JsonSerializeError { err: e }.into())?,
        )]))
    }
}

/// Result type for the [Prune Volumes API](../struct.Docker.html#method.prune_volumes)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct PruneVolumesResults {
    pub volumes_deleted: Option<Vec<String>>,
    pub space_reclaimed: u64,
}

impl Docker {
    /// ---
    ///
    /// # List volumes
    ///
    /// # Arguments
    ///
    ///  - [List Volumes Options](volume/struct.ListVolumesOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A [List Volumes Results](volume/struct.ListVolumesResults.html) struct, wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::volume::ListVolumesOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("dangling", vec!("1"));
    ///
    /// let options = ListVolumesOptions {
    ///     filters: filters,
    /// };
    ///
    /// docker.list_volumes(Some(options));
    /// ```
    pub async fn list_volumes<T>(
        &self,
        options: Option<ListVolumesOptions<T>>,
    ) -> Result<ListVolumesResults, Error>
    where
        T: AsRef<str> + Eq + Hash + Serialize,
    {
        let url = "/volumes";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Create Volume
    ///
    /// Create a new volume.
    ///
    /// # Arguments
    ///
    ///  - [Create Volume Options](volume/struct.CreateVolumeOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A [Volume Api](volume/struct.VolumeAPI.html) struct, wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::volume::CreateVolumeOptions;
    ///
    /// use std::default::Default;
    ///
    /// let config = CreateVolumeOptions {
    ///     name: "certs",
    ///     ..Default::default()
    /// };
    ///
    /// docker.create_volume(config);
    /// ```
    pub async fn create_volume<T>(&self, config: CreateVolumeOptions<T>) -> Result<VolumeAPI, Error>
    where
        T: AsRef<str> + Eq + Hash + Serialize,
    {
        let url = "/volumes/create";

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::POST),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Inspect a Volume
    ///
    /// # Arguments
    ///
    ///  - Volume name as a string slice.
    ///
    /// # Returns
    ///
    ///  - A [Volume API](volume/struct.VolumeAPI.html) struct, wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.inspect_volume("my_volume_name");
    /// ```
    pub async fn inspect_volume(&self, volume_name: &str) -> Result<VolumeAPI, Error> {
        let url = format!("/volumes/{}", volume_name);

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
    /// # Remove a Volume
    ///
    /// # Arguments
    ///
    ///  - Volume name as a string slice.
    ///
    /// # Arguments
    ///
    ///  - [Remove Volume Options](volume/struct.RemoveVolumeOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::volume::RemoveVolumeOptions;
    ///
    /// let options = RemoveVolumeOptions {
    ///     force: true,
    /// };
    ///
    /// docker.remove_volume("my_volume_name", Some(options));
    /// ```
    pub async fn remove_volume(
        &self,
        volume_name: &str,
        options: Option<RemoveVolumeOptions>,
    ) -> Result<(), Error> {
        let url = format!("/volumes/{}", volume_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Prune Volumes
    ///
    /// Delete unused volumes.
    ///
    /// # Arguments
    ///
    ///  - A [Prune Volumes Options](volume/struct.PruneVolumesOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A [Prune Volumes Results](volume/struct.PruneVolumesResults.html) struct.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::volume::PruneVolumesOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("label", vec!("maintainer=some_maintainer"));
    ///
    /// let options = PruneVolumesOptions {
    ///     filters: filters,
    /// };
    ///
    /// docker.prune_volumes(Some(options));
    /// ```
    pub async fn prune_volumes<T, K, V>(
        &self,
        options: Option<T>,
    ) -> Result<PruneVolumesResults, Error>
    where
        T: PruneVolumesQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = "/volumes/prune";

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }
}
