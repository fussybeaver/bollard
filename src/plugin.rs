//! Plugin API: manage Docker plugins

use super::Docker;
use crate::{docker::BodyType, errors::Error};
use bytes::Bytes;
use http::request::Builder;
use http_body_util::Full;
use hyper::Method;
use serde_derive::Serialize;
use std::{collections::HashMap, hash::Hash};

pub use crate::models::*;

/// Parameters used in the [List Plugins API](Docker::list_plugins())
///
/// ## Examples
///
/// ```rust
/// # use std::collections::HashMap;
/// # use std::default::Default;
/// use bollard::plugin::ListPluginsOptions;
///
/// let mut filters = HashMap::new();
/// filters.insert("capability", vec!["volumedriver"]);
///
/// ListPluginsOptions {
///     filters,
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ListPluginsOptions<T>
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    /// Filters to process on the plugin list.
    /// Available filters:
    ///  - `capability=<capability name>`
    ///  - `enable=<true>|<false>`
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

/// Parameters used in the [Remove Plugin API](Docker::remove_plugin())
///
/// ## Examples
///
/// ```rust
/// use bollard::plugin::RemovePluginOptions;
///
/// RemovePluginOptions {
///     force: true,
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
pub struct RemovePluginOptions {
    /// Disable the plugin before removing. This may result in issues if the plugin is in use by a container.
    pub force: bool,
}

/// Parameters used in the [Enable Plugin API](Docker::enable_plugin())
///
/// ## Examples
///
/// ```rust
/// use bollard::plugin::EnablePluginOptions;
///
/// EnablePluginOptions {
///     timeout: 30,
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
pub struct EnablePluginOptions {
    /// Set the HTTP client timeout (in seconds).
    pub timeout: u64,
}

/// Parameters used in the [Disable Plugin API](Docker::disable_plugin())
///
/// ## Examples
///
/// ```rust
/// use bollard::plugin::DisablePluginOptions;
///
/// DisablePluginOptions {
///     force: true,
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
pub struct DisablePluginOptions {
    /// Force disable a plugin even if still in use.
    pub force: bool,
}

/// Parameters used in the [Get Plugin Privileges API](Docker::get_plugin_privileges())
///
/// ## Examples
///
/// ```rust
/// use bollard::plugin::GetPluginPrivilegesOptions;
///
/// GetPluginPrivilegesOptions {
///     remote: "vieux/sshfs:latest",
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GetPluginPrivilegesOptions<T>
where
    T: Into<String> + serde::ser::Serialize,
{
    /// The name of the plugin. The `:latest` tag is optional, and is the default if omitted.
    pub remote: T,
}

impl Docker {
    /// ---
    ///
    /// # List Plugins
    ///
    /// Returns a list of plugins.
    ///
    /// # Arguments
    ///
    ///  - Optional [ListPluginsOptions](ListPluginsOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [Plugin](Plugin), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::plugin::ListPluginsOptions;
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("capability", vec!["volumedriver"]);
    ///
    /// let options = Some(ListPluginsOptions {
    ///     filters,
    ///     ..Default::default()
    /// });
    ///
    /// docker.list_plugins(options);
    /// ```
    pub async fn list_plugins<T>(
        &self,
        options: Option<ListPluginsOptions<T>>,
    ) -> Result<Vec<Plugin>, Error>
    where
        T: Into<String> + Eq + Hash + serde::ser::Serialize,
    {
        let url = "/plugins";

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
    /// # Inspect Plugin
    ///
    /// Inspect a plugin.
    ///
    /// # Arguments
    ///
    ///  - Plugin name as a string slice.
    ///
    /// # Returns
    ///
    ///  - [Plugin](Plugin), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.inspect_plugin("vieux/sshfs:latest");
    /// ```
    pub async fn inspect_plugin(&self, plugin_name: &str) -> Result<Plugin, Error> {
        let url = format!("/plugins/{plugin_name}/json");

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
    /// # Remove Plugin
    ///
    /// Remove a plugin.
    ///
    /// # Arguments
    ///
    ///  - Plugin name as a string slice.
    ///  - Optional [RemovePluginOptions](RemovePluginOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [Plugin](Plugin), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::plugin::RemovePluginOptions;
    ///
    /// let options = Some(RemovePluginOptions {
    ///     force: true,
    /// });
    ///
    /// docker.remove_plugin("vieux/sshfs:latest", options);
    /// ```
    pub async fn remove_plugin(
        &self,
        plugin_name: &str,
        options: Option<RemovePluginOptions>,
    ) -> Result<Plugin, Error> {
        let url = format!("/plugins/{plugin_name}");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Enable Plugin
    ///
    /// Enable a plugin.
    ///
    /// # Arguments
    ///
    ///  - Plugin name as a string slice.
    ///  - Optional [EnablePluginOptions](EnablePluginOptions) struct.
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
    /// use bollard::plugin::EnablePluginOptions;
    ///
    /// let options = Some(EnablePluginOptions {
    ///     timeout: 30,
    /// });
    ///
    /// docker.enable_plugin("vieux/sshfs:latest", options);
    /// ```
    pub async fn enable_plugin(
        &self,
        plugin_name: &str,
        options: Option<EnablePluginOptions>,
    ) -> Result<(), Error> {
        let url = format!("/plugins/{plugin_name}/enable");

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
    /// # Disable Plugin
    ///
    /// Disable a plugin.
    ///
    /// # Arguments
    ///
    ///  - Plugin name as a string slice.
    ///  - Optional [DisablePluginOptions](DisablePluginOptions) struct.
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
    /// use bollard::plugin::DisablePluginOptions;
    ///
    /// let options = Some(DisablePluginOptions {
    ///     force: true,
    /// });
    ///
    /// docker.disable_plugin("vieux/sshfs:latest", options);
    /// ```
    pub async fn disable_plugin(
        &self,
        plugin_name: &str,
        options: Option<DisablePluginOptions>,
    ) -> Result<(), Error> {
        let url = format!("/plugins/{plugin_name}/disable");

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
    /// # Get Plugin Privileges
    ///
    /// Get the list of privileges required by a plugin.
    ///
    /// # Arguments
    ///
    ///  - [GetPluginPrivilegesOptions](GetPluginPrivilegesOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [PluginPrivilege](PluginPrivilege), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::plugin::GetPluginPrivilegesOptions;
    ///
    /// let options = GetPluginPrivilegesOptions {
    ///     remote: "vieux/sshfs:latest",
    /// };
    ///
    /// docker.get_plugin_privileges(options);
    /// ```
    pub async fn get_plugin_privileges<T>(
        &self,
        options: GetPluginPrivilegesOptions<T>,
    ) -> Result<Vec<PluginPrivilege>, Error>
    where
        T: Into<String> + serde::ser::Serialize,
    {
        let url = "/plugins/privileges";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            Some(options),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }
}
