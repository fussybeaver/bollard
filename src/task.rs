//! Tasks API: A task is a container running on a swarm. It is the atomic scheduling unit of swarm. Swarm mode must be enabled for these endpoints to work.
#![allow(deprecated)]

use bollard_stubs::models::Task;
use bytes::Bytes;
use futures_core::Stream;
use http_body_util::Full;
use serde::Serialize;
use std::{collections::HashMap, hash::Hash};

use crate::{container::LogOutput, docker::BodyType, errors::Error, Docker};
use http::{request::Builder, Method};

/// Parameters used in the [List Tasks API](super::Docker::list_tasks())
///
/// ## Examples
///
/// ```rust
/// use bollard::task::ListTasksOptions;
///
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("label", vec!["maintainer=some_maintainer"]);
///
/// ListTasksOptions {
///     filters
/// };
/// ```
///
/// ```rust
/// # use bollard::task::ListTasksOptions;
/// # use std::default::Default;
///
/// ListTasksOptions::<&str> {
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::ListTasksOptions and associated ListTasksOptionsBuilder"
)]
pub struct ListTasksOptions<T>
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    /// JSON encoded value of the filters (a `map[string][]string`) to process on the tasks list.
    ///
    /// Available filters:
    ///  - `desired-state=["running"|"shutdown"|"accepted"]`: Matches the desired state of the task.
    ///  - `id=<task-id>`: Matches the id of the task.
    ///  - `label=<key>` or `label=<key>=<value>`: Matches a task label.
    ///  - `name=<task-name>`: Matches all or part of a task name.
    ///  - `node=<node-id>`: Matches all or part of a node id or name.
    ///  - `service=<service-name>`: Matches all or part of a service name.
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

impl<T> From<ListTasksOptions<T>> for crate::query_parameters::ListTasksOptions
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    fn from(opts: ListTasksOptions<T>) -> Self {
        crate::query_parameters::ListTasksOptionsBuilder::default()
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

impl Docker {
    /// ---
    ///
    /// # List Tasks
    ///
    /// # Arguments
    ///
    ///  - Optional [List Tasks Options](ListTasksOptions) struct.
    ///
    /// # Returns
    ///
    ///  - A vector of [Task](Task) struct, wrapped in a
    ///    Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::task::ListTasksOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut list_tasks_filters = HashMap::new();
    /// list_tasks_filters.insert("label", vec!["my-task-label"]);
    ///
    /// let config = ListTasksOptions {
    ///     filters: list_tasks_filters,
    /// };
    ///
    /// docker.list_tasks(Some(config));
    /// ```
    pub async fn list_tasks(
        &self,
        options: Option<impl Into<crate::query_parameters::ListTasksOptions>>,
    ) -> Result<Vec<Task>, Error> {
        let url = "/tasks";

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
    /// # Inspect a Task
    ///
    /// # Arguments
    ///
    ///  - Task id as a string slice.
    ///
    /// # Returns
    ///
    ///  - A [Models](Task) struct, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.inspect_task("my_task_id");
    /// ```
    pub async fn inspect_task(&self, task_id: &str) -> Result<Task, Error> {
        let url = format!("/tasks/{task_id}");

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
    /// # Get Task Logs
    ///
    /// Get `stdout` and `stderr` logs from a task.
    ///
    /// # Arguments
    ///
    ///  - Task id as a string slice.
    ///  - Optional [Logs Options](crate::container::LogsOptions) struct.
    ///
    /// # Returns
    ///
    ///  - A Stream of [Log Output](LogOutput) results.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::LogsOptions;
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(LogsOptions::<String>{
    ///     stdout: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.task_logs("my-task-id", options);
    /// ```
    pub fn task_logs(
        &self,
        task_id: &str,
        options: Option<impl Into<crate::query_parameters::LogsOptions>>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> {
        let url = format!("/tasks/{task_id}/logs");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_stream_string(req)
    }
}
