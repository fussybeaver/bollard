//! Secret API: manage and inspect docker secrets within a swarm
#![allow(deprecated)]

pub use crate::models::*;

use super::Docker;
use crate::{docker::BodyType, errors::Error};
use bytes::Bytes;
use http::request::Builder;
use http_body_util::Full;
use hyper::Method;
use serde_derive::Serialize;
use std::{collections::HashMap, hash::Hash};

/// Parameters used in the [List Secret API](super::Docker::list_secrets())
///
/// ## Examples
///
/// ```rust
/// # use std::collections::HashMap;
/// # use std::default::Default;
/// use bollard::secret::ListSecretsOptions;
///
/// let mut filters = HashMap::new();
/// filters.insert("name", vec!["my-secret-name"]);
///
/// ListSecretsOptions{
///     filters,
/// };
/// ```
///
/// ```rust
/// # use bollard::secret::ListSecretsOptions;
/// # use std::default::Default;
///
/// let options: ListSecretsOptions<&str> = Default::default();
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::ListSecretsOptions and associated ListSecretsOptionsBuilder"
)]
pub struct ListSecretsOptions<T>
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    /// Filters to process on the secret list, encoded as JSON. Available filters:
    ///  - `id`=`<ID>` a secret's ID
    ///  - `label`=`key` or `label`=`"key=value"` of a secret label
    ///  - `name`=`<name>` a secret's name
    ///  - `names`=`<name>` a multiple secret's name comma separated
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

impl<T> From<ListSecretsOptions<T>> for crate::query_parameters::ListSecretsOptions
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    fn from(opts: ListSecretsOptions<T>) -> Self {
        crate::query_parameters::ListSecretsOptionsBuilder::default()
            .filters(
                &opts
                    .filters
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into_iter().map(T::into).collect()))
                    .collect(),
            )
            .build()
    }
}

/// Parameters used in the [Update Secret API](Docker::update_secret())
///
/// ## Examples
///
/// ```rust
/// use bollard::secret::UpdateSecretOptions;
///
/// UpdateSecretOptions{
///     version: 1234,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::UpdateSecretOptions and associated UpdateSecretOptionsBuilder"
)]
pub struct UpdateSecretOptions {
    /// The version number of the secret object being updated. This is required to avoid conflicting writes. This version number should be the value as currently set on the secret before the update.
    pub version: u64,
}

impl From<UpdateSecretOptions> for crate::query_parameters::UpdateSecretOptions {
    fn from(opts: UpdateSecretOptions) -> Self {
        crate::query_parameters::UpdateSecretOptionsBuilder::default()
            .version(opts.version as i64)
            .build()
    }
}

impl Docker {
    /// ---
    ///
    /// # List Secrets
    ///
    /// Returns a list of secrets.
    ///
    /// # Arguments
    ///
    ///  - Optional [ListSecretsOptions](ListSecretsOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [Secret](Secret), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::secret::ListSecretsOptions;
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("label", vec!["secret-label=label-value"]);
    ///
    /// let options = Some(ListSecretsOptions{
    ///     filters,
    ///     ..Default::default()
    /// });
    ///
    /// docker.list_secrets(options);
    /// ```
    pub async fn list_secrets(
        &self,
        options: Option<impl Into<crate::query_parameters::ListSecretsOptions>>,
    ) -> Result<Vec<Secret>, Error> {
        let url = "/secrets";

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
    /// # Create Secret
    ///
    /// Create new secret on the docker swarm.
    ///
    /// # Arguments
    ///
    ///  - [SecretSpec](SecretSpec) struct.
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
    /// use bollard::secret::SecretSpec;
    ///
    /// use base64;
    ///
    /// let secret_spec = SecretSpec {
    ///     name: Some(String::from("secret-name")),
    ///     data: Some(base64::engine::general_purpose::STANDARD.encode("secret-data")),
    ///     ..Default::default()
    /// };
    ///
    /// docker.create_secret(secret_spec);
    /// ```
    pub async fn create_secret(&self, secret_spec: SecretSpec) -> Result<IdResponse, Error> {
        let url = "/secrets/create";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            None::<String>,
            Docker::serialize_payload(Some(secret_spec)),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Inspect Secret
    ///
    /// Inspect a secret.
    ///
    /// # Arguments
    ///
    ///  - Secret id or name as a string slice.
    ///
    /// # Returns
    ///
    ///  - [Secret](Secret), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.inspect_secret("secret-id");
    /// docker.inspect_secret("secret-name");
    /// ```
    pub async fn inspect_secret(&self, secret_id: &str) -> Result<Secret, Error> {
        let url = format!("/secrets/{secret_id}");

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
    /// # Delete Secret
    ///
    /// Delete a secret, fails when more than one service use that secret..
    ///
    /// # Arguments
    ///
    ///  - Secret id or name as a string slice.
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
    /// docker.delete_secret("secret-id");
    /// docker.delete_secret("secret-name");
    /// ```
    pub async fn delete_secret(&self, secret_id: &str) -> Result<(), Error> {
        let url = format!("/secrets/{secret_id}");

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
    /// # Update Secret
    ///
    /// Update an existing secret,
    /// fails when more than one service use that secret or trying update data.
    ///
    /// # Arguments
    ///
    ///  - Secret id or name as a string slice.
    ///  - [SecretSpec](SecretSpec) struct.
    ///  - [UpdateSecretOptions](UpdateSecretOptions) struct.
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
    /// use bollard::secret::UpdateSecretOptions;
    ///
    /// let result = async move {
    ///     let existing = docker.inspect_secret("my-secret").await?;
    ///     let version = existing.version.unwrap().index.unwrap();
    ///     let mut spec = existing.spec.unwrap().clone();
    ///
    ///     let mut labels = HashMap::new();
    ///     labels.insert(String::from("secret-label"), String::from("label-value"));
    ///     spec.labels = Some(labels.clone());
    ///
    ///     let options = UpdateSecretOptions { version };
    ///
    ///     docker.update_secret("my-secret", spec, options).await
    /// };
    /// ```
    pub async fn update_secret(
        &self,
        secret_id: &str,
        secret_spec: SecretSpec,
        options: impl Into<crate::query_parameters::UpdateSecretOptions>,
    ) -> Result<(), Error> {
        let url = format!("/secrets/{secret_id}/update");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Some(options.into()),
            Docker::serialize_payload(Some(secret_spec)),
        );

        self.process_into_unit(req).await
    }
}
