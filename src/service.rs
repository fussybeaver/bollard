use super::Docker;
use crate::errors::Error;
use crate::errors::ErrorKind::JsonSerializeError;
use crate::service_models::Service;
use crate::service_models::ServiceSpec;
use arrayvec::ArrayVec;
use http::request::Builder;
use hyper::{Body, Method};
use serde_json;
use std::{collections::HashMap, hash::Hash};

/// Parameters used in the [List Service API](../struct.Docker.html#method.list_services)
///
/// ## Examples
///
/// ```rust
/// use bollard::container::ListServicesOptions;
///
/// use std::collections::HashMap;
/// use std::default::Default;
///
/// let mut filters = HashMap::new();
/// filters.insert("mode", vec!("global"));
///
/// ListServicesOptions{
///     filters: filters,
///     ..Default::default()
/// };
/// ```
///
/// ```rust
/// # use bollard::container::ListContainersOptions;
/// # use std::default::Default;
/// ListContainersOptions::<String>{
///     ..Default::default()
/// };
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
    /// hidden field to restrict to usage with `..Default::default()`
    /// so that additional fields could be added later without breaking api
    hidden: (),
}

#[allow(missing_docs)]
/// Trait providing implementations for [List Services Options](struct.ListContainersOptions.html)
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
            serde_json::to_string(&self.filters).map_err(|e| JsonSerializeError { err: e })?,
        )]))
    }
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
    ///  - Vector of [APIServices](service/struct.APIServices.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::{ListServicesOptions};
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

    pub async fn create_service<T, K>(
        &self,
        service_spec: ServiceSpec,
        options: Option<T>,
    ) -> Result<Vec<Service>, Error>
    where
        T: ListServicesQueryParams<K, String>,
        K: AsRef<str>,
    {
        let url = "/services/create";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            Docker::transpose_option(options.map(|o| o.into_array())),
            Docker::serialize_payload(Some(service_spec)),
        );

        self.process_into_value(req).await
    }
}
