//! Service API: manage and inspect docker services within a swarm

pub use crate::models::*;

use super::Docker;
use crate::auth::DockerCredentials;
use crate::errors::Error;
use http::header::CONTENT_TYPE;
use http::request::Builder;
use hyper::{Body, Method};
use serde::ser::Serialize;

use std::{collections::HashMap, hash::Hash};

/// Parameters used in the [List Service API](super::Docker::list_services())
///
/// ## Examples
///
/// ```rust
/// # use std::collections::HashMap;
/// # use std::default::Default;
/// use bollard::service::ListServicesOptions;
///
/// let mut filters = HashMap::new();
/// filters.insert("mode", vec!["global"]);
///
/// ListServicesOptions{
///     filters,
/// };
/// ```
///
/// ```rust
/// # use bollard::service::ListServicesOptions;
/// # use std::default::Default;
///
/// let options: ListServicesOptions<&str> = Default::default();
/// ```
#[derive(Debug, Clone, Default, Serialize)]
pub struct ListServicesOptions<T>
where
    T: Into<String> + Eq + Hash + Serialize,
{
    /// Filters to process on the service list, encoded as JSON. Available filters:
    ///  - `id`=`<ID>` a services's ID
    ///  - `label`=`key` or `label`=`"key=value"` of a service label
    ///  - `mode`=`["replicated"|"global"] a service's scheduling mode
    ///  - `name`=`<name>` a services's name
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

/// Parameters used in the [Inspect Service API](Docker::inspect_service())
///
/// ## Examples
///
/// ```rust
/// use bollard::service::InspectServiceOptions;
///
/// InspectServiceOptions{
///     insert_defaults: true,
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectServiceOptions {
    /// Fill empty fields with default values.
    pub insert_defaults: bool,
}

/// Parameters used in the [Update Service API](Docker::update_service())
///
/// ## Examples
///
/// ```rust
/// use bollard::service::UpdateServiceOptions;
///
/// UpdateServiceOptions{
///     version: 1234,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateServiceOptions {
    /// The version number of the service object being updated. This is required to avoid conflicting writes. This version number should be the value as currently set on the service before the update.
    pub version: u64,
    /// If the X-Registry-Auth header is not specified, this parameter indicates whether to use registry authorization credentials from the current or the previous spec.
    #[serde(serialize_with = "serialize_registry_auth_from")]
    pub registry_auth_from: bool,
    /// Set to this parameter to true to cause a server-side rollback to the previous service spec. The supplied spec will be ignored in this case.
    #[serde(serialize_with = "serialize_rollback")]
    pub rollback: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn serialize_registry_auth_from<S>(
    registry_auth_from: &bool,
    s: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(if *registry_auth_from {
        "previous-spec"
    } else {
        "spec"
    })
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_rollback<S>(rollback: &bool, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(if *rollback { "previous" } else { "" })
}

impl Docker {
    /// ---
    ///
    /// # List Services
    ///
    /// Returns a list of services.
    ///
    /// # Arguments
    ///
    ///  - Optional [ListServicesOptions](ListServicesOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [Services](Service), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::service::ListServicesOptions;
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("mode", vec!["global"]);
    ///
    /// let options = Some(ListServicesOptions{
    ///     filters,
    ///     ..Default::default()
    /// });
    ///
    /// docker.list_services(options);
    /// ```
    pub async fn list_services<T>(
        &self,
        options: Option<ListServicesOptions<T>>,
    ) -> Result<Vec<Service>, Error>
    where
        T: Into<String> + Eq + Hash + Serialize,
    {
        let url = "/services";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            options,
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Create Service
    ///
    /// Dispatch a new service on the docker swarm
    ///
    /// # Arguments
    ///
    ///  - [ServiceSpec](ServiceSpec) struct.
    ///  - Optional [Docker Credentials](DockerCredentials) struct.
    ///
    /// # Returns
    ///
    ///  - A [Service Create Response](ServiceCreateResponse) struct,
    ///  wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # use std::collections::HashMap;
    /// # use std::default::Default;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::service::{
    ///     ServiceSpec,
    ///     ServiceSpecMode,
    ///     ServiceSpecModeReplicated,
    ///     TaskSpec,
    ///     TaskSpecContainerSpec
    /// };
    ///
    /// let service = ServiceSpec {
    ///     name: Some(String::from("my-service")),
    ///     mode: Some(ServiceSpecMode {
    ///         replicated: Some(ServiceSpecModeReplicated {
    ///             replicas: Some(2)
    ///         }),
    ///         ..Default::default()
    ///     }),
    ///     task_template: Some(TaskSpec {
    ///         container_spec: Some(TaskSpecContainerSpec {
    ///             image: Some(String::from("hello-world")),
    ///             ..Default::default()
    ///         }),
    ///         ..Default::default()
    ///     }),
    ///     ..Default::default()
    /// };
    /// let credentials = None;
    ///
    /// docker.create_service(service, credentials);
    /// ```
    pub async fn create_service(
        &self,
        service_spec: ServiceSpec,
        credentials: Option<DockerCredentials>,
    ) -> Result<ServiceCreateResponse, Error> {
        let url = "/services/create";

        match serde_json::to_string(&credentials.unwrap_or_else(|| DockerCredentials {
            ..Default::default()
        })) {
            Ok(ser_cred) => {
                let req = self.build_request(
                    url,
                    Builder::new()
                        .method(Method::POST)
                        .header(CONTENT_TYPE, "application/json")
                        .header("X-Registry-Auth", base64::encode(&ser_cred)),
                    None::<String>,
                    Docker::serialize_payload(Some(service_spec)),
                );

                self.process_into_value(req).await
            }
            Err(e) => Err(e.into()),
        }
    }

    /// ---
    ///
    /// # Inspect Service
    ///
    /// Inspect a service.
    ///
    /// # Arguments
    ///
    ///  - Service name or id as a string slice.
    ///  - Optional [Inspect Service Options](InspectServiceOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [Service](Service), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::service::InspectServiceOptions;
    ///
    /// let options = Some(InspectServiceOptions{
    ///     insert_defaults: true,
    /// });
    ///
    /// docker.inspect_service("my-service", options);
    /// ```
    pub async fn inspect_service(
        &self,
        service_name: &str,
        options: Option<InspectServiceOptions>,
    ) -> Result<Service, Error> {
        let url = format!("/services/{}", service_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options,
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Delete Service
    ///
    /// Delete a service.
    ///
    /// # Arguments
    ///
    /// - Service name or id as a string slice.
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
    /// docker.delete_service("my-service");
    /// ```
    pub async fn delete_service(&self, service_name: &str) -> Result<(), Error> {
        let url = format!("/services/{}", service_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            None::<String>,
            Ok(Body::empty()),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Update Service
    ///
    /// Update an existing service
    ///
    /// # Arguments
    ///
    ///  - Service name or id as a string slice.
    ///  - [ServiceSpec](ServiceSpec) struct.
    ///  - [UpdateServiceOptions](UpdateServiceOptions) struct.
    ///  - Optional [Docker Credentials](DockerCredentials) struct.
    ///
    /// # Returns
    ///
    ///  - A [Service Update Response](ServiceUpdateResponse) struct,
    ///  wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::service::{
    ///     InspectServiceOptions,
    ///     ServiceSpec,
    ///     ServiceSpecMode,
    ///     ServiceSpecModeReplicated,
    ///     TaskSpec,
    ///     TaskSpecContainerSpec,
    ///     UpdateServiceOptions,
    /// };
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let result = async move {
    ///     let service_name = "my-service";
    ///     let current_version = docker.inspect_service(
    ///         service_name,
    ///         None::<InspectServiceOptions>
    ///     ).await?.version.unwrap().index.unwrap();
    ///     let service = ServiceSpec {
    ///         mode: Some(ServiceSpecMode {
    ///             replicated: Some(ServiceSpecModeReplicated {
    ///                 replicas: Some(0)
    ///             }),
    ///             ..Default::default()
    ///         }),
    ///         ..Default::default()
    ///     };
    ///     let options = UpdateServiceOptions {
    ///         version: current_version,
    ///         ..Default::default()
    ///     };
    ///     let credentials = None;
    ///
    ///     docker.update_service("my-service", service, options, credentials).await
    /// };
    /// ```
    pub async fn update_service(
        &self,
        service_name: &str,
        service_spec: ServiceSpec,
        options: UpdateServiceOptions,
        credentials: Option<DockerCredentials>,
    ) -> Result<ServiceUpdateResponse, Error> {
        let url = format!("/services/{}/update", service_name);

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
                    Some(options),
                    Docker::serialize_payload(Some(service_spec)),
                );

                self.process_into_value(req).await
            }
            Err(e) => Err(e.into()),
        }
    }
}
