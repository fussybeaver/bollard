//! Plugin API: manage Docker plugins

#![allow(deprecated)]

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
#[deprecated(
    since = "0.19.0",
    note = "Use `crate::query_parameters::ListPluginsOptions` instead"
)]
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

impl<T> From<ListPluginsOptions<T>> for crate::query_parameters::ListPluginsOptions
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    fn from(opts: ListPluginsOptions<T>) -> Self {
        let filters: HashMap<String, Vec<String>> = opts
            .filters
            .into_iter()
            .map(|(k, v)| (k.into(), v.into_iter().map(Into::into).collect()))
            .collect();
        crate::query_parameters::ListPluginsOptionsBuilder::default()
            .filters(&filters)
            .build()
    }
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
#[deprecated(
    since = "0.19.0",
    note = "Use `crate::query_parameters::RemovePluginOptions` instead"
)]
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
pub struct RemovePluginOptions {
    /// Disable the plugin before removing. This may result in issues if the plugin is in use by a container.
    pub force: bool,
}

impl From<RemovePluginOptions> for crate::query_parameters::RemovePluginOptions {
    fn from(opts: RemovePluginOptions) -> Self {
        crate::query_parameters::RemovePluginOptionsBuilder::default()
            .force(opts.force)
            .build()
    }
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
#[deprecated(
    since = "0.19.0",
    note = "Use `crate::query_parameters::EnablePluginOptions` instead"
)]
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
pub struct EnablePluginOptions {
    /// Set the HTTP client timeout (in seconds).
    pub timeout: u64,
}

impl From<EnablePluginOptions> for crate::query_parameters::EnablePluginOptions {
    fn from(opts: EnablePluginOptions) -> Self {
        crate::query_parameters::EnablePluginOptionsBuilder::default()
            .timeout(opts.timeout as i32)
            .build()
    }
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
#[deprecated(
    since = "0.19.0",
    note = "Use `crate::query_parameters::DisablePluginOptions` instead"
)]
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
pub struct DisablePluginOptions {
    /// Force disable a plugin even if still in use.
    pub force: bool,
}

impl From<DisablePluginOptions> for crate::query_parameters::DisablePluginOptions {
    fn from(opts: DisablePluginOptions) -> Self {
        crate::query_parameters::DisablePluginOptionsBuilder::default()
            .force(opts.force)
            .build()
    }
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
#[deprecated(
    since = "0.19.0",
    note = "Use `crate::query_parameters::GetPluginPrivilegesOptions` instead"
)]
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GetPluginPrivilegesOptions<T>
where
    T: Into<String> + serde::ser::Serialize,
{
    /// The name of the plugin. The `:latest` tag is optional, and is the default if omitted.
    pub remote: T,
}

impl<T> From<GetPluginPrivilegesOptions<T>> for crate::query_parameters::GetPluginPrivilegesOptions
where
    T: Into<String> + serde::ser::Serialize,
{
    fn from(opts: GetPluginPrivilegesOptions<T>) -> Self {
        let remote: String = opts.remote.into();
        crate::query_parameters::GetPluginPrivilegesOptionsBuilder::default()
            .remote(&remote)
            .build()
    }
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
    ///  - Optional [ListPluginsOptions](crate::query_parameters::ListPluginsOptions) struct.
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
    /// use bollard::query_parameters::ListPluginsOptionsBuilder;
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("capability", vec!["volumedriver"]);
    ///
    /// let filters: HashMap<String, Vec<String>> = filters.into_iter().map(|(k, v)| (k.to_string(), v.into_iter().map(String::from).collect())).collect();
    /// let options = ListPluginsOptionsBuilder::default()
    ///     .filters(&filters)
    ///     .build();
    ///
    /// docker.list_plugins(Some(options));
    /// ```
    pub async fn list_plugins(
        &self,
        options: Option<impl Into<crate::query_parameters::ListPluginsOptions>>,
    ) -> Result<Vec<Plugin>, Error> {
        let url = "/plugins";

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
    ///  - Optional [RemovePluginOptions](crate::query_parameters::RemovePluginOptions) struct.
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
    /// use bollard::query_parameters::RemovePluginOptionsBuilder;
    ///
    /// let options = RemovePluginOptionsBuilder::default()
    ///     .force(true)
    ///     .build();
    ///
    /// docker.remove_plugin("vieux/sshfs:latest", Some(options));
    /// ```
    pub async fn remove_plugin(
        &self,
        plugin_name: &str,
        options: Option<impl Into<crate::query_parameters::RemovePluginOptions>>,
    ) -> Result<Plugin, Error> {
        let url = format!("/plugins/{plugin_name}");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            options.map(Into::into),
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
    ///  - Optional [EnablePluginOptions](crate::query_parameters::EnablePluginOptions) struct.
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
    /// use bollard::query_parameters::EnablePluginOptionsBuilder;
    ///
    /// let options = EnablePluginOptionsBuilder::default()
    ///     .timeout(30)
    ///     .build();
    ///
    /// docker.enable_plugin("vieux/sshfs:latest", Some(options));
    /// ```
    pub async fn enable_plugin(
        &self,
        plugin_name: &str,
        options: Option<impl Into<crate::query_parameters::EnablePluginOptions>>,
    ) -> Result<(), Error> {
        let url = format!("/plugins/{plugin_name}/enable");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options.map(Into::into),
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
    ///  - Optional [DisablePluginOptions](crate::query_parameters::DisablePluginOptions) struct.
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
    /// use bollard::query_parameters::DisablePluginOptionsBuilder;
    ///
    /// let options = DisablePluginOptionsBuilder::default()
    ///     .force(true)
    ///     .build();
    ///
    /// docker.disable_plugin("vieux/sshfs:latest", Some(options));
    /// ```
    pub async fn disable_plugin(
        &self,
        plugin_name: &str,
        options: Option<impl Into<crate::query_parameters::DisablePluginOptions>>,
    ) -> Result<(), Error> {
        let url = format!("/plugins/{plugin_name}/disable");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options.map(Into::into),
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
    ///  - [GetPluginPrivilegesOptions](crate::query_parameters::GetPluginPrivilegesOptions) struct.
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
    /// use bollard::query_parameters::GetPluginPrivilegesOptionsBuilder;
    ///
    /// let options = GetPluginPrivilegesOptionsBuilder::default()
    ///     .remote("vieux/sshfs:latest")
    ///     .build();
    ///
    /// docker.get_plugin_privileges(options);
    /// ```
    pub async fn get_plugin_privileges(
        &self,
        options: impl Into<crate::query_parameters::GetPluginPrivilegesOptions>,
    ) -> Result<Vec<PluginPrivilege>, Error> {
        let url = "/plugins/privileges";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            Some(options.into()),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }
}
