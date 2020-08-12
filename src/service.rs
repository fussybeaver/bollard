//! Service API: manage and inspect docker services within a swarm

pub use crate::models::*;

use super::Docker;
use crate::auth::DockerCredentials;
use crate::docker::{FALSE_STR, TRUE_STR};
use crate::errors::Error;
use arrayvec::ArrayVec;
use http::header::CONTENT_TYPE;
use http::request::Builder;
use hyper::{Body, Method};
use serde_json;
use std::{collections::HashMap, hash::Hash};

/// Parameters used in the [List Service API](../struct.Docker.html#method.list_services)
///
/// ## Examples
///
/// ```rust
/// # use std::collections::HashMap;
/// # use std::default::Default;
/// use bollard::service::ListServicesOptions;
///
/// let mut filters = HashMap::new();
/// filters.insert("mode", vec!("global"));
///
/// ListServicesOptions{
///     filters: filters,
/// };
/// ```
///
/// ```rust
/// # use bollard::service::ListServicesOptions;
/// # use std::default::Default;
///
/// let options: ListServicesOptions<&str> = Default::default();
/// ```
#[derive(Debug, Clone, Default)]
pub struct ListServicesOptions<T>
where
    T: AsRef<str> + Eq + Hash,
{
    /// Filters to process on the service list, encoded as JSON. Available filters:
    ///  - `id`=`<ID>` a services's ID
    ///  - `label`=`key` or `label`=`"key=value"` of a service label
    ///  - `mode`=`["replicated"|"global"] a service's scheduling mode
    ///  - `name`=`<name>` a services's name
    pub filters: HashMap<T, Vec<T>>,
}

#[allow(missing_docs)]
/// Trait providing implementations for [List Services Options](struct.ListServicesOptions.html)
/// struct.
pub trait ListServicesQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a, T: AsRef<str> + Eq + Hash> ListServicesQueryParams<&'a str, String>
    for ListServicesOptions<T>
where
    T: ::serde::Serialize,
{
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 1]>, Error> {
        Ok(ArrayVec::from([(
            "filters",
            serde_json::to_string(&self.filters)?,
        )]))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ServiceCreateResponse {
    /// The ID of the created service.
    #[serde(rename = "ID")]
    pub id: Option<String>,

    /// Optional warning message
    pub warning: Option<String>,
}

/// Parameters used in the [Inspect Service API](../struct.Docker.html#method.inspect_service)
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
#[derive(Debug, Copy, Clone, Default)]
pub struct InspectServiceOptions {
    /// Fill empty fields with default values.
    pub insert_defaults: bool,
}

/// Trait providing implementations for [Inspect Service Options](struct.InspectServiceOptions.html).
#[allow(missing_docs)]
pub trait InspectServiceQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 1]>, Error>;
}

impl<'a> InspectServiceQueryParams<&'a str, &'a str> for InspectServiceOptions {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, &'a str); 1]>, Error> {
        Ok(ArrayVec::from([(
            "insertDefaults",
            if self.insert_defaults {
                TRUE_STR
            } else {
                FALSE_STR
            },
        )]))
    }
}

/// Parameters used in the [Update Service API](../struct.Docker.html#method.update_service)
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
#[derive(Debug, Copy, Clone, Default)]
pub struct UpdateServiceOptions {
    /// The version number of the service object being updated. This is required to avoid conflicting writes. This version number should be the value as currently set on the service before the update.
    pub version: u64,
    /// If the X-Registry-Auth header is not specified, this parameter indicates whether to use registry authorization credentials from the current or the previous spec.
    pub registry_auth_from_previous: bool,
    /// Set to this parameter to true to cause a server-side rollback to the previous service spec. The supplied spec will be ignored in this case.
    pub rollback: bool,
}

/// Trait providing implementations for [Update Service Options](struct.UpdateServiceOptions.html).
#[allow(missing_docs)]
pub trait UpdateServiceQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<ArrayVec<[(K, V); 3]>, Error>;
}

impl<'a> UpdateServiceQueryParams<&'a str, String> for UpdateServiceOptions {
    fn into_array(self) -> Result<ArrayVec<[(&'a str, String); 3]>, Error> {
        Ok(ArrayVec::from([
            ("version", self.version.to_string()),
            (
                "registryAuthFrom",
                if self.registry_auth_from_previous {
                    "previous-spec"
                } else {
                    "spec"
                }
                .to_string(),
            ),
            (
                "rollback",
                if self.rollback { "previous" } else { "" }.to_string(),
            ),
        ]))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ServiceUpdateResponse {
    /// Optional warning message
    pub warning: Option<String>,
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
    ///  - Optional [ListServicesOptions](service/struct.ListServicesOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [Services](models/struct.Service.html), wrapped in a Future.
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
    /// filters.insert("mode", vec!("global"));
    ///
    /// let options = Some(ListServicesOptions{
    ///     filters: filters,
    ///     ..Default::default()
    /// });
    ///
    /// docker.list_services(options);
    /// ```
    pub async fn list_services<T, K>(&self, options: Option<T>) -> Result<Vec<Service>, Error>
    where
        T: ListServicesQueryParams<K, String>,
        K: AsRef<str>,
    {
        let url = "/services";

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
    /// # Create Service
    ///
    /// Dispatch a new service on the docker swarm
    ///
    /// # Arguments
    ///
    ///  - [ServiceSpec](models/struct.ServiceSpec.html) struct.
    ///  - Optional [Docker Credentials](auth/struct.DockerCredentials.html) struct.
    ///
    /// # Returns
    ///
    ///  - A [Service Create Response](models/struct.ServiceCreateResponse.html) struct,
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
                let req = self.build_request::<_, String, String>(
                    url,
                    Builder::new()
                        .method(Method::POST)
                        .header(CONTENT_TYPE, "application/json")
                        .header("X-Registry-Auth", base64::encode(&ser_cred)),
                    Ok(None::<ArrayVec<[(_, _); 0]>>),
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
    ///  - Optional [Inspect Service Options](service/struct.InspectServiceOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - [Service](models/struct.Service.html), wrapped in a Future.
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
    pub async fn inspect_service<T, K, V>(
        &self,
        service_name: &str,
        options: Option<T>,
    ) -> Result<Service, Error>
    where
        T: InspectServiceQueryParams<K, V>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = format!("/services/{}", service_name);

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

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::DELETE),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
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
    ///  - [ServiceSpec](models/struct.ServiceSpec.html) struct.
    ///  - [UpdateServiceOptions](service/struct.UpdateServiceOptions.html) struct.
    ///  - Optional [Docker Credentials](auth/struct.DockerCredentials.html) struct.
    ///
    /// # Returns
    ///
    ///  - A [Service Update Response](models/struct.ServiceUpdateResponse.html) struct,
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
    pub async fn update_service<T, K>(
        &self,
        service_name: &str,
        service_spec: ServiceSpec,
        options: T,
        credentials: Option<DockerCredentials>,
    ) -> Result<ServiceUpdateResponse, Error>
    where
        T: UpdateServiceQueryParams<K, String>,
        K: AsRef<str>,
    {
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
                    Docker::transpose_option(Some(options.into_array())),
                    Docker::serialize_payload(Some(service_spec)),
                );

                self.process_into_value(req).await
            }
            Err(e) => Err(e.into()),
        }
    }
}
