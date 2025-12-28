//! Plugin API: manage Docker plugins

use super::Docker;
use crate::{docker::BodyType, errors::Error};
use bytes::Bytes;
use http::request::Builder;
use http_body_util::Full;
use hyper::Method;

pub use crate::models::*;

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
        options: Option<crate::query_parameters::ListPluginsOptions>,
    ) -> Result<Vec<Plugin>, Error> {
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
        options: Option<crate::query_parameters::RemovePluginOptions>,
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
        options: Option<crate::query_parameters::EnablePluginOptions>,
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
        options: Option<crate::query_parameters::DisablePluginOptions>,
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
        options: crate::query_parameters::GetPluginPrivilegesOptions,
    ) -> Result<Vec<PluginPrivilege>, Error> {
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
