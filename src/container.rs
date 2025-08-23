//! Container API: run docker containers and manage their lifecycle

use futures_core::Stream;
use futures_util::{StreamExt, TryStreamExt};
use http::header::{CONNECTION, CONTENT_TYPE, UPGRADE};
use http::request::Builder;
use http_body_util::Full;
use hyper::{body::Bytes, Method};
use tokio::io::AsyncWrite;
use tokio_util::codec::FramedRead;

use std::fmt;
use std::pin::Pin;

use super::Docker;
use crate::docker::BodyType;
use crate::errors::Error;
use crate::models::*;
use crate::read::NewlineLogOutputDecoder;

/// Results type for the [Attach Container API](Docker::attach_container())
pub struct AttachContainerResults {
    /// [Log Output](LogOutput) enum, wrapped in a Stream.
    pub output: Pin<Box<dyn Stream<Item = Result<LogOutput, Error>> + Send>>,
    /// Byte writer to container
    pub input: Pin<Box<dyn AsyncWrite + Send>>,
}

impl fmt::Debug for AttachContainerResults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AttachContainerResults")
    }
}

/// Result type for the [Logs API](Docker::logs())
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub enum LogOutput {
    StdErr { message: Bytes },
    StdOut { message: Bytes },
    StdIn { message: Bytes },
    Console { message: Bytes },
}

impl fmt::Display for LogOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match &self {
            LogOutput::StdErr { message } => message,
            LogOutput::StdOut { message } => message,
            LogOutput::StdIn { message } => message,
            LogOutput::Console { message } => message,
        };
        write!(f, "{}", String::from_utf8_lossy(message))
    }
}

impl AsRef<[u8]> for LogOutput {
    fn as_ref(&self) -> &[u8] {
        match self {
            LogOutput::StdErr { message } => message.as_ref(),
            LogOutput::StdOut { message } => message.as_ref(),
            LogOutput::StdIn { message } => message.as_ref(),
            LogOutput::Console { message } => message.as_ref(),
        }
    }
}

impl LogOutput {
    /// Get the raw bytes of the output
    pub fn into_bytes(self) -> Bytes {
        match self {
            LogOutput::StdErr { message } => message,
            LogOutput::StdOut { message } => message,
            LogOutput::StdIn { message } => message,
            LogOutput::Console { message } => message,
        }
    }
}

impl Docker {
    /// ---
    ///
    /// # List Containers
    ///
    /// Returns a list of containers.
    ///
    /// # Arguments
    ///
    ///  - Optional [ListContainersOptions](ListContainersOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Vector of [ContainerSummary](ContainerSummary), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::ListContainersOptions;
    ///
    /// use std::collections::HashMap;
    /// use std::default::Default;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("health", vec!["unhealthy"]);
    ///
    /// let options = Some(ListContainersOptions{
    ///     all: true,
    ///     filters,
    ///     ..Default::default()
    /// });
    ///
    /// docker.list_containers(options);
    /// ```
    pub async fn list_containers(
        &self,
        options: Option<impl Into<crate::query_parameters::ListContainersOptions>>,
    ) -> Result<Vec<ContainerSummary>, Error> {
        let url = "/containers/json";

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
    /// # Create Container
    ///
    /// Prepares a container for a subsequent start operation.
    ///
    /// # Arguments
    ///
    ///  - Optional [Create Container Options](CreateContainerOptions) struct.
    ///  - Container [Config](Config) struct.
    ///
    /// # Returns
    ///
    ///  - [ContainerCreateResponse](ContainerCreateResponse), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::{CreateContainerOptions, Config};
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(CreateContainerOptions{
    ///     name: "my-new-container",
    ///     platform: None,
    /// });
    ///
    /// let config = Config {
    ///     image: Some("hello-world"),
    ///     cmd: Some(vec!["/hello"]),
    ///     ..Default::default()
    /// };
    ///
    /// docker.create_container(options, config);
    /// ```
    pub async fn create_container(
        &self,
        options: Option<crate::query_parameters::CreateContainerOptions>,
        config: ContainerCreateBody,
    ) -> Result<ContainerCreateResponse, Error> {
        let url = "/containers/create";
        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            options,
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Start Container
    ///
    /// Starts a container, after preparing it with the [Create Container
    /// API](struct.Docker.html#method.create_container).
    ///
    /// # Arguments
    ///
    ///  - Container name as a string slice.
    ///  - Optional [Start Container Options](StartContainerOptions) struct.
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
    /// use bollard::container::StartContainerOptions;
    ///
    /// docker.start_container("hello-world", None::<StartContainerOptions<String>>);
    /// ```
    pub async fn start_container(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::StartContainerOptions>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/start");

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
    /// # Stop Container
    ///
    /// Stops a container.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - Optional [Stop Container Options](StopContainerOptions) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// use bollard::container::StopContainerOptions;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// let options = Some(StopContainerOptions{
    ///     t: 30,
    /// });
    ///
    /// docker.stop_container("hello-world", options);
    /// ```
    pub async fn stop_container(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::StopContainerOptions>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/stop");

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
    /// # Remove Container
    ///
    /// Remove a container.
    ///
    /// # Arguments
    ///
    /// - Container name as a string slice.
    /// - Optional [Remove Container Options](RemoveContainerOptions) struct.
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
    /// use bollard::container::RemoveContainerOptions;
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(RemoveContainerOptions{
    ///     force: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.remove_container("hello-world", options);
    /// ```
    pub async fn remove_container(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::RemoveContainerOptions>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Wait Container
    ///
    /// Wait for a container to stop. This is a non-blocking operation, the resulting stream will
    /// end when the container stops.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - Optional [Wait Container Options](WaitContainerOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [ContainerWaitResponse](ContainerWaitResponse), wrapped in a
    ///    Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::container::WaitContainerOptions;
    ///
    /// let options = Some(WaitContainerOptions{
    ///     condition: "not-running",
    /// });
    ///
    /// docker.wait_container("hello-world", options);
    /// ```
    pub fn wait_container(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::WaitContainerOptions>,
    ) -> impl Stream<Item = Result<ContainerWaitResponse, Error>> {
        let url = format!("/containers/{container_name}/wait");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_stream(req).map(|res| match res {
            Ok(ContainerWaitResponse {
                status_code: code,
                error:
                    Some(ContainerWaitExitError {
                        message: Some(error),
                    }),
            }) if code > 0 => Err(Error::DockerContainerWaitError { error, code }),
            Ok(ContainerWaitResponse {
                status_code: code,
                error: None,
            }) if code > 0 => Err(Error::DockerContainerWaitError {
                error: String::new(),
                code,
            }),
            v => v,
        })
    }

    /// ---
    ///
    /// # Attach Container
    ///
    /// Attach to a container to read its output or send it input. You can attach to the
    /// same container multiple times and you can reattach to containers that have been detached.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - Optional [Attach Container Options](AttachContainerOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [AttachContainerResults](AttachContainerResults) wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::container::AttachContainerOptions;
    ///
    /// let options = Some(AttachContainerOptions::<String>{
    ///     stdin: Some(true),
    ///     stdout: Some(true),
    ///     stderr: Some(true),
    ///     stream: Some(true),
    ///     logs: Some(true),
    ///     detach_keys: Some("ctrl-c".to_string()),
    /// });
    ///
    /// docker.attach_container("hello-world", options);
    /// ```
    pub async fn attach_container(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::AttachContainerOptions>,
    ) -> Result<AttachContainerResults, Error> {
        let url = format!("/containers/{container_name}/attach");

        let req = self.build_request(
            &url,
            Builder::new()
                .method(Method::POST)
                .header(CONNECTION, "Upgrade")
                .header(UPGRADE, "tcp"),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        let (read, write) = self.process_upgraded(req).await?;
        let log = FramedRead::new(read, NewlineLogOutputDecoder::new(true)).map_err(|e| e.into());

        Ok(AttachContainerResults {
            output: Box::pin(log),
            input: Box::pin(write),
        })
    }

    /// ---
    ///
    /// # Resize container tty
    ///
    /// Resize the container's TTY.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - [Resize Container Tty Options](ResizeContainerTtyOptions) struct.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::container::ResizeContainerTtyOptions;
    ///
    /// let options = ResizeContainerTtyOptions {
    ///     width: 50,
    ///     height: 20,
    /// };
    ///
    /// docker.resize_container_tty("hello-world", options);
    /// ```
    pub async fn resize_container_tty(
        &self,
        container_name: &str,
        options: crate::query_parameters::ResizeContainerTTYOptions,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/resize");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Some(options),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Restart Container
    ///
    /// Restart a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - Optional [Restart Container Options](RestartContainerOptions) struct.
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
    /// use bollard::container::RestartContainerOptions;
    ///
    /// let options = Some(RestartContainerOptions{
    ///     t: 30,
    /// });
    ///
    /// docker.restart_container("postgres", options);
    /// ```
    pub async fn restart_container(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::RestartContainerOptions>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/restart");

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
    /// # Inspect Container
    ///
    /// Inspect a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as a string slice.
    ///  - Optional [Inspect Container Options](InspectContainerOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [ContainerInspectResponse](ContainerInspectResponse), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::InspectContainerOptions;
    ///
    /// let options = Some(InspectContainerOptions{
    ///     size: false,
    /// });
    ///
    /// docker.inspect_container("hello-world", options);
    /// ```
    pub async fn inspect_container(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::InspectContainerOptions>,
    ) -> Result<ContainerInspectResponse, Error> {
        let url = format!("/containers/{container_name}/json");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Top Processes
    ///
    /// List processes running inside a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - Optional [Top Options](TopOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [ContainerTopResponse](ContainerTopResponse), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::TopOptions;
    ///
    /// let options = Some(TopOptions{
    ///     ps_args: "aux",
    /// });
    ///
    /// docker.top_processes("fussybeaver/uhttpd", options);
    /// ```
    pub async fn top_processes(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::TopOptions>,
    ) -> Result<ContainerTopResponse, Error> {
        let url = format!("/containers/{container_name}/top");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Logs
    ///
    /// Get container logs.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - Optional [Logs Options](LogsOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [Log Output](LogOutput) enum, wrapped in a
    ///    Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::container::LogsOptions;
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(LogsOptions::<String>{
    ///     stdout: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.logs("hello-world", options);
    /// ```
    pub fn logs(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::LogsOptions>,
    ) -> impl Stream<Item = Result<LogOutput, Error>> {
        let url = format!("/containers/{container_name}/logs");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_stream_string(req)
    }

    /// ---
    ///
    /// # Container Changes
    ///
    /// Get changes on a container's filesystem.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///
    /// # Returns
    ///
    ///  - An Option of Vector of [File System Change](FilesystemChange) structs, wrapped in a
    ///    Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.container_changes("hello-world");
    /// ```
    pub async fn container_changes(
        &self,
        container_name: &str,
    ) -> Result<Option<Vec<FilesystemChange>>, Error> {
        let url = format!("/containers/{container_name}/changes");

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
    /// # Stats
    ///
    /// Get container stats based on resource usage.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - Optional [Stats Options](StatsOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [Stats](Stats) struct, wrapped in a
    ///    Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::container::StatsOptions;
    ///
    /// let options = Some(StatsOptions{
    ///     stream: false,
    ///     one_shot: true,
    /// });
    ///
    /// docker.stats("hello-world", options);
    /// ```
    pub fn stats(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::StatsOptions>,
    ) -> impl Stream<Item = Result<ContainerStatsResponse, Error>> {
        let url = format!("/containers/{container_name}/stats");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_stream(req)
    }

    /// ---
    ///
    /// # Kill Container
    ///
    /// Kill a container.
    ///
    /// # Arguments
    ///
    /// - Container name as string slice.
    /// - Optional [Kill Container Options](KillContainerOptions) struct.
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
    /// use bollard::container::KillContainerOptions;
    ///
    /// let options = Some(KillContainerOptions{
    ///     signal: "SIGINT",
    /// });
    ///
    /// docker.kill_container("postgres", options);
    /// ```
    pub async fn kill_container(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::KillContainerOptions>,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/kill");

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
    /// # Update Container
    ///
    /// Update a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - [Update Container Options](UpdateContainerOptions) struct.
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
    /// use bollard::container::UpdateContainerOptions;
    /// use std::default::Default;
    ///
    /// let config = UpdateContainerOptions::<String> {
    ///     memory: Some(314572800),
    ///     memory_swap: Some(314572800),
    ///     ..Default::default()
    /// };
    ///
    /// docker.update_container("postgres", config);
    /// ```
    pub async fn update_container(
        &self,
        container_name: &str,
        config: ContainerUpdateBody,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/update");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Rename Container
    ///
    /// Rename a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - [Rename Container Options](RenameContainerOptions) struct
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
    /// use bollard::container::RenameContainerOptions;
    ///
    /// let required = RenameContainerOptions {
    ///     name: "my_new_container_name"
    /// };
    ///
    /// docker.rename_container("hello-world", required);
    /// ```
    pub async fn rename_container(
        &self,
        container_name: &str,
        options: crate::query_parameters::RenameContainerOptions,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/rename");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Some(options),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Pause Container
    ///
    /// Use the cgroups freezer to suspend all processes in a container.
    ///
    /// # Arguments
    ///
    ///  - Container name as a string slice.
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
    /// docker.pause_container("postgres");
    /// ```
    pub async fn pause_container(&self, container_name: &str) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/pause");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Unpause Container
    ///
    /// Resume a container which has been paused.
    ///
    /// # Arguments
    ///
    ///  - Container name as a string slice.
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
    /// docker.unpause_container("postgres");
    /// ```
    pub async fn unpause_container(&self, container_name: &str) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/unpause");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Prune Containers
    ///
    /// Delete stopped containers.
    ///
    /// # Arguments
    ///
    ///  - Optional [Prune Containers Options](PruneContainersOptions) struct.
    ///
    /// # Returns
    ///
    ///  - [Container Prune Response](ContainerPruneResponse) struct, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::PruneContainersOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut filters = HashMap::new();
    /// filters.insert("until", vec!["10m"]);
    ///
    /// let options = Some(PruneContainersOptions{
    ///     filters
    /// });
    ///
    /// docker.prune_containers(options);
    /// ```
    pub async fn prune_containers(
        &self,
        options: Option<crate::query_parameters::PruneContainersOptions>,
    ) -> Result<ContainerPruneResponse, Error> {
        let url = "/containers/prune";

        let req = self.build_request(
            url,
            Builder::new().method(Method::POST),
            options,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Stream Upload To Container
    ///
    /// Stream an upload of a tar archive to be extracted to a path in the filesystem of container
    /// id.
    ///
    /// # Arguments
    ///
    ///  - Optional [Upload To Container Options](UploadToContainerOptions) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use bollard::Docker;
    /// use bollard::container::UploadToContainerOptions;
    /// use futures_util::{StreamExt, TryFutureExt};
    /// use tokio::fs::File;
    /// use tokio_util::io::ReaderStream;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let options = Some(UploadToContainerOptions{
    ///     path: "/opt",
    ///     ..Default::default()
    /// });
    ///
    /// let file = File::open("tarball.tar.gz")
    ///     .map_ok(ReaderStream::new)
    ///     .try_flatten_stream()
    ///     .map(|x|x.expect("failed to stream file"));
    ///
    /// docker
    ///     .upload_to_container_streaming("my-container", options, file)
    ///     .await
    ///     .expect("upload failed");
    /// # }
    /// ```
    #[inline(always)]
    #[deprecated(
        since = "0.19.0",
        note = "This method is refactored into upload_to_container"
    )]
    pub async fn upload_to_container_streaming(
        &self,
        container_name: &str,
        options: Option<crate::query_parameters::UploadToContainerOptions>,
        tar: impl Stream<Item = Bytes> + Send + 'static,
    ) -> Result<(), Error> {
        self.upload_to_container(container_name, options, crate::body_stream(tar))
            .await
    }

    /// ---
    ///
    /// # Upload To Container
    ///
    /// Upload a tar archive to be extracted to a path in the filesystem of container id.
    ///
    /// # Arguments
    ///
    ///  - Optional [Upload To Container Options](UploadToContainerOptions) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// Uploading a tarball
    ///
    /// ```rust,no_run
    /// # use bollard::Docker;
    /// use bollard::container::UploadToContainerOptions;
    /// use bollard::body_full;
    /// use std::fs::File;
    /// use std::io::Read;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let options = Some(UploadToContainerOptions{
    ///     path: "/opt",
    ///     ..Default::default()
    /// });
    ///
    /// let mut file = File::open("tarball.tar.gz").unwrap();
    /// let mut contents = Vec::new();
    /// file.read_to_end(&mut contents).unwrap();
    ///
    /// docker
    ///     .upload_to_container("my-container", options, body_full(contents.into()))
    ///     .await
    ///     .expect("upload failed");
    /// # }
    /// ```
    /// Uploading a stream
    ///
    /// ```rust,no_run
    /// # use bollard::Docker;
    /// use bollard::container::UploadToContainerOptions;
    /// use bollard::body_try_stream;
    /// use futures_util::{StreamExt, TryFutureExt};
    /// use tokio::fs::File;
    /// use tokio_util::io::ReaderStream;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// let options = Some(UploadToContainerOptions{
    ///     path: "/opt",
    ///     ..Default::default()
    /// });
    ///
    /// let file = File::open("tarball.tar.gz")
    ///     .map_ok(ReaderStream::new)
    ///     .try_flatten_stream();
    ///
    /// docker
    ///     .upload_to_container("my-container", options, body_try_stream(file))
    ///     .await
    ///     .expect("upload failed");
    /// # }
    /// ```
    pub async fn upload_to_container(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::UploadToContainerOptions>>,
        tar: BodyType,
    ) -> Result<(), Error> {
        let url = format!("/containers/{container_name}/archive");

        let req = self.build_request(
            &url,
            Builder::new()
                .method(Method::PUT)
                .header(CONTENT_TYPE, "application/x-tar"),
            options.map(Into::into),
            Ok(tar),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Download From Container
    ///
    /// Get a tar archive of a resource in the filesystem of container id.
    ///
    /// # Arguments
    ///
    ///  - [Download From Container Options](DownloadFromContainerOptions) struct.
    ///
    /// # Returns
    ///
    ///  - Tar archive compressed with one of the following algorithms: identity (no compression),
    ///    gzip, bzip2, xz. [Hyper Body](hyper::body::Body).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// use bollard::container::DownloadFromContainerOptions;
    ///
    /// let options = Some(DownloadFromContainerOptions{
    ///     path: "/opt",
    /// });
    ///
    /// docker.download_from_container("my-container", options);
    /// ```
    pub fn download_from_container(
        &self,
        container_name: &str,
        options: Option<impl Into<crate::query_parameters::DownloadFromContainerOptions>>,
    ) -> impl Stream<Item = Result<Bytes, Error>> {
        let url = format!("/containers/{container_name}/archive");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_body(req)
    }

    /// ---
    ///
    /// # Export Container
    ///
    /// Get a tarball containing the filesystem contents of a container.
    ///
    /// See the [Docker API documentation](https://docs.docker.com/engine/api/v1.40/#operation/ContainerExport)
    /// for more information.
    /// # Arguments
    /// - The `container_name` string referring to an individual container
    ///
    /// # Returns
    ///  - An uncompressed TAR archive
    pub fn export_container(
        &self,
        container_name: &str,
    ) -> impl Stream<Item = Result<Bytes, Error>> {
        let url = format!("/containers/{container_name}/export");
        let req = self.build_request(
            &url,
            Builder::new()
                .method(Method::GET)
                .header(CONTENT_TYPE, "application/json"),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );
        self.process_into_body(req)
    }
}

#[cfg(not(windows))]
#[cfg(test)]
mod tests {

    use bollard_stubs::models::ContainerCreateBody;
    use futures_util::TryStreamExt;
    use yup_hyper_mock::HostToReplyConnector;

    use crate::{Docker, API_DEFAULT_VERSION};

    #[tokio::test]
    async fn test_container_wait_with_error() {
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            String::from("http://127.0.0.1"),
            "HTTP/1.1 200 OK\r\nServer:mock1\r\nContent-Type:application/json\r\n\r\n{\"Error\":null,\"StatusCode\":1}".to_string(),
        );

        let docker =
            Docker::connect_with_mock(connector, "127.0.0.1".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let result = &docker
            .wait_container("wait_container_test", None)
            .try_collect::<Vec<_>>()
            .await;

        assert!(matches!(
            result,
            Err(crate::errors::Error::DockerContainerWaitError { code: _, error: _ })
        ));
    }

    #[tokio::test]
    async fn test_output_non_json_error() {
        let mut connector = HostToReplyConnector::default();
        connector.m.insert(
            String::from("http://127.0.0.1"),
            "HTTP/1.1 200 OK\r\nServer:mock1\r\nContent-Type:plain/text\r\n\r\nthis is not json"
                .to_string(),
        );
        let docker =
            Docker::connect_with_mock(connector, "127.0.0.1".to_string(), 5, API_DEFAULT_VERSION)
                .unwrap();

        let host_config = bollard_stubs::models::HostConfig {
            mounts: Some(vec![bollard_stubs::models::Mount {
                target: Some(String::from("/tmp")),
                source: Some(String::from("./tmp")),
                typ: Some(bollard_stubs::models::MountTypeEnum::BIND),
                consistency: Some(String::from("default")),
                ..Default::default()
            }]),
            ..Default::default()
        };

        let result = &docker
            .create_container(
                Some(
                    crate::query_parameters::CreateContainerOptionsBuilder::default()
                        .name("mount_volume_container_failure_test")
                        .build(),
                ),
                ContainerCreateBody {
                    image: Some(String::from("some_image")),
                    host_config: Some(host_config),
                    ..Default::default()
                },
            )
            .await;

        println!("{result:#?}");

        assert!(matches!(
            result,
            Err(crate::errors::Error::JsonDataError {
                message: _,
                column: 2
            })
        ));
    }
}
