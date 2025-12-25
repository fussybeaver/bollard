//! Config API: manage and inspect docker configs within a swarm

pub use crate::models::*;

use super::Docker;
use crate::{docker::BodyType, errors::Error};
use bytes::Bytes;
use http::request::Builder;
use http_body_util::Full;
use hyper::Method;
use serde_derive::Serialize;
use std::{collections::HashMap, hash::Hash};

/// Parameters used in the [List Config API](super::Docker::list_configs())
///
/// ## Examples
///
/// ```rust
/// # use std::collections::HashMap;
/// # use std::default::Default;
/// use bollard::config::ListConfigsOptions;
///
/// let mut filters = HashMap::new();
/// filters.insert("name", vec!["my-config-name"]);
///
/// ListConfigsOptions{
///     filters,
/// };
/// ```
///
/// ```rust
/// # use bollard::config::ListConfigsOptions;
/// # use std::default::Default;
///
/// let options: ListConfigsOptions<&str> = Default::default();
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ListConfigsOptions<T>
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    /// Filters to process on the config list, encoded as JSON. Available filters:
    ///  - `id`=`<ID>` a config's ID
    ///  - `label`=`key` or `label`=`"key=value"` of a config label
    ///  - `name`=`<name>` a config's name
    ///  - `names`=`<name>` a multiple config's name comma separated
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

/// Parameters used in the [Update Config API](Docker::update_config())
///
/// ## Examples
///
/// ```rust
/// use bollard::config::UpdateConfigOptions;
///
/// UpdateConfigOptions{
///     version: 1234,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
pub struct UpdateConfigOptions {
    /// The version number of the config object being updated. This is required to avoid conflicting writes. This version number should be the value as currently set on the config before the update.
    pub version: u64,
}

impl Docker {
    /// ---
    ///
    /// # List Configs
    ///
    /// Returns a list of configs.
    ///
    /// # Arguments
    ///
    ///  - Optional [ListConfigsOptions](ListConfigsOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [Config](Config), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::config::ListConfigsOptions;
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("label", vec!["config-label=label-value"]);
    ///
    /// let options = Some(ListConfigsOptions{
    ///     filters,
    ///     ..Default::default()
    /// });
    ///
    /// docker.list_configs(options);
    /// ```
    pub async fn list_configs<T>(
        &self,
        options: Option<ListConfigsOptions<T>>,
    ) -> Result<Vec<Config>, Error>
    where
        T: Into<String> + Eq + Hash + serde::ser::Serialize,
    {
        let url = "/configs";

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
    /// # Create Config
    ///
    /// Create a new config on the docker swarm.
    ///
    /// # Arguments
    ///
    ///  - [ConfigSpec](ConfigSpec) struct.
    ///
    /// # Returns
    ///
    ///  - A [IdResponse](IdResponse) wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # use std::default::Default;
    /// # use base64::Engine;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::config::ConfigSpec;
    ///
    /// use base64;
    ///
    /// let config_spec = ConfigSpec {
    ///     name: Some(String::from("config-name")),
    ///     data: Some(base64::engine::general_purpose::STANDARD.encode("config-data")),
    ///     ..Default::default()
    /// };
    ///
    /// docker.create_config(config_spec);
    /// ```
    pub async fn create_config(&self, config_spec: ConfigSpec) -> Result<IdResponse, Error> {
        let url = "/configs/create";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            None::<String>,
            Docker::serialize_payload(Some(config_spec)),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Inspect Config
    ///
    /// Inspect a config.
    ///
    /// # Arguments
    ///
    ///  - Config id or name as a string slice.
    ///
    /// # Returns
    ///
    ///  - [Config](Config), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.inspect_config("config-id");
    /// docker.inspect_config("config-name");
    /// ```
    pub async fn inspect_config(&self, config_id: &str) -> Result<Config, Error> {
        let url = format!("/configs/{config_id}");

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
    /// # Delete Config
    ///
    /// Delete a config.
    ///
    /// # Arguments
    ///
    ///  - Config id or name as a string slice.
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
    /// docker.delete_config("config-id");
    /// docker.delete_config("config-name");
    /// ```
    pub async fn delete_config(&self, config_id: &str) -> Result<(), Error> {
        let url = format!("/configs/{config_id}");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Update Config
    ///
    /// Update an existing config.
    ///
    /// # Arguments
    ///
    ///  - Config id or name as a string slice.
    ///  - [ConfigSpec](ConfigSpec) struct.
    ///  - [UpdateConfigOptions](UpdateConfigOptions) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use std::collections::HashMap;
    /// use bollard::config::UpdateConfigOptions;
    ///
    /// let result = async move {
    ///     let existing = docker.inspect_config("my-config").await?;
    ///     let version = existing.version.unwrap().index.unwrap();
    ///     let mut spec = existing.spec.unwrap().clone();
    ///
    ///     let mut labels = HashMap::new();
    ///     labels.insert(String::from("config-label"), String::from("label-value"));
    ///     spec.labels = Some(labels.clone());
    ///
    ///     let options = UpdateConfigOptions { version };
    ///
    ///     docker.update_config("my-config", spec, options).await
    /// };
    /// ```
    pub async fn update_config(
        &self,
        config_id: &str,
        config_spec: ConfigSpec,
        options: UpdateConfigOptions,
    ) -> Result<(), Error> {
        let url = format!("/configs/{config_id}/update");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Some(options),
            Docker::serialize_payload(Some(config_spec)),
        );

        self.process_into_unit(req).await
    }
}
