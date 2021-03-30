//! Exec API: Run new commands inside running containers

use http::header::{CONNECTION, UPGRADE};
use http::request::Builder;
use hyper::Body;
use hyper::Method;
use serde::ser::Serialize;

use super::Docker;

use crate::container::LogOutput;
use crate::errors::Error;
use crate::models::ExecInspectResponse;
use crate::read::NewlineLogOutputDecoder;
use futures_core::Stream;
use std::fmt::{Debug, Formatter};
use std::pin::Pin;
use tokio::io::AsyncWrite;
use tokio_util::codec::FramedRead;

/// Exec configuration used in the [Create Exec API](Docker::create_exec())
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateExecOptions<T>
where
    T: Into<String> + Serialize,
{
    /// Attach to `stdin` of the exec command.
    pub attach_stdin: Option<bool>,
    /// Attach to stdout of the exec command.
    pub attach_stdout: Option<bool>,
    /// Attach to stderr of the exec command.
    pub attach_stderr: Option<bool>,
    /// Allocate a pseudo-TTY.
    pub tty: Option<bool>,
    /// Override the key sequence for detaching a container. Format is a single character `[a-Z]`
    /// or `ctrl-<value>` where `<value>` is one of: `a-z`, `@`, `^`, `[`, `,` or `_`.
    pub detach_keys: Option<T>,
    /// A list of environment variables in the form `["VAR=value", ...].`
    pub env: Option<Vec<T>>,
    /// Command to run, as a string or array of strings.
    pub cmd: Option<Vec<T>>,
    /// Runs the exec process with extended privileges.
    pub privileged: Option<bool>,
    /// The user, and optionally, group to run the exec process inside the container. Format is one
    /// of: `user`, `user:group`, `uid`, or `uid:gid`.
    pub user: Option<T>,
    /// The working directory for the exec process inside the container.
    pub working_dir: Option<T>,
}

/// Result type for the [Create Exec API](Docker::create_exec())
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct CreateExecResults {
    pub id: String,
}

/// Exec configuration used in the [Create Exec API](Docker::create_exec())
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StartExecOptions {
    /// Detach from the command.
    pub detach: bool,
}

/// Result type for the [Start Exec API](Docker::start_exec())
#[allow(missing_docs)]
pub enum StartExecResults {
    Attached {
        output: Pin<Box<dyn Stream<Item = Result<LogOutput, Error>> + Send>>,
        input: Pin<Box<dyn AsyncWrite + Send>>,
    },
    Detached,
}

impl Debug for StartExecResults {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StartExecResults::Attached { .. } => write!(f, "StartExecResults::Attached"),
            StartExecResults::Detached => write!(f, "StartExecResults::Detached"),
        }
    }
}

/// Resize configuration used in the [Resize Exec API](Docker::resize_exec())
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ResizeExecOptions {
    /// Height of the TTY session in characters
    #[serde(rename = "h")]
    pub height: u16,
    /// Width of the TTY session in characters
    #[serde(rename = "w")]
    pub width: u16,
}

impl Docker {
    /// ---
    ///
    /// # Create Exec
    ///
    /// Run a command inside a running container.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - [Create Exec Options](CreateExecOptions) struct.
    ///
    /// # Returns
    ///
    ///  - A [Create Exec Results](CreateExecResults) struct, wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::exec::CreateExecOptions;
    ///
    /// use std::default::Default;
    ///
    /// let config = CreateExecOptions {
    ///     cmd: Some(vec!["ps", "-ef"]),
    ///     attach_stdout: Some(true),
    ///     ..Default::default()
    /// };
    ///
    /// docker.create_exec("hello-world", config);
    /// ```
    pub async fn create_exec<T>(
        &self,
        container_name: &str,
        config: CreateExecOptions<T>,
    ) -> Result<CreateExecResults, Error>
    where
        T: Into<String> + Serialize,
    {
        let url = format!("/containers/{}/exec", container_name);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            None::<String>,
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Start Exec
    ///
    /// Starts a previously set up exec instance. If detach is true, this endpoint returns
    /// immediately after starting the command.
    ///
    /// # Arguments
    ///
    ///  - The ID of the previously created exec configuration.
    ///
    /// # Returns
    ///
    ///  - [Log Output](LogOutput) enum, wrapped in a Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// # use bollard::exec::CreateExecOptions;
    /// # use std::default::Default;
    ///
    /// # let config = CreateExecOptions {
    /// #     cmd: Some(vec!["ps", "-ef"]),
    /// #     attach_stdout: Some(true),
    /// #     ..Default::default()
    /// # };
    ///
    /// async {
    ///     let message = docker.create_exec("hello-world", config).await.unwrap();
    ///     use bollard::exec::StartExecOptions;
    ///     docker.start_exec(&message.id, None::<StartExecOptions>);
    /// };
    /// ```
    pub async fn start_exec(
        &self,
        exec_id: &str,
        config: Option<StartExecOptions>,
    ) -> Result<StartExecResults, Error> {
        let url = format!("/exec/{}/start", exec_id);

        match config {
            Some(StartExecOptions { detach: true, .. }) => {
                let req = self.build_request(
                    &url,
                    Builder::new().method(Method::POST),
                    None::<String>,
                    Docker::serialize_payload(config),
                );

                self.process_into_unit(req).await?;
                Ok(StartExecResults::Detached)
            }
            _ => {
                let req = self.build_request(
                    &url,
                    Builder::new()
                        .method(Method::POST)
                        .header(CONNECTION, "Upgrade")
                        .header(UPGRADE, "tcp"),
                    None::<String>,
                    Docker::serialize_payload(config.or_else(|| {
                        Some(StartExecOptions {
                            ..Default::default()
                        })
                    })),
                );

                let (read, write) = self.process_upgraded(req).await?;

                let log = FramedRead::new(read, NewlineLogOutputDecoder::new());
                Ok(StartExecResults::Attached {
                    output: Box::pin(log),
                    input: Box::pin(write),
                })
            }
        }
    }

    /// ---
    ///
    /// # Inspect Exec
    ///
    /// Return low-level information about an exec instance.
    ///
    /// # Arguments
    ///
    ///  - The ID of the previously created exec configuration.
    ///
    /// # Returns
    ///
    ///  - An [Exec Inspect Response](ExecInspectResponse) struct, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// # use bollard::exec::CreateExecOptions;
    /// # use std::default::Default;
    ///
    /// # let config = CreateExecOptions {
    /// #     cmd: Some(vec!["ps", "-ef"]),
    /// #     attach_stdout: Some(true),
    /// #     ..Default::default()
    /// # };
    ///
    /// async {
    ///     let message = docker.create_exec("hello-world", config).await.unwrap();
    ///     docker.inspect_exec(&message.id);
    /// };
    /// ```
    pub async fn inspect_exec(&self, exec_id: &str) -> Result<ExecInspectResponse, Error> {
        let url = format!("/exec/{}/json", exec_id);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(Body::empty()),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Resize Exec
    ///
    /// Resize the TTY session used by an exec instance. This endpoint only works if `tty` was specified as part of creating and starting the exec instance.
    ///
    /// # Arguments
    ///
    ///  - The ID of the previously created exec configuration.
    ///  - [Resize Exec Options](ResizeExecOptions) struct.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    /// #
    /// # use bollard::exec::{CreateExecOptions, ResizeExecOptions};
    /// # use std::default::Default;
    /// #
    /// # let config = CreateExecOptions {
    /// #     cmd: Some(vec!["ps", "-ef"]),
    /// #     attach_stdout: Some(true),
    /// #     ..Default::default()
    /// # };
    /// #
    /// async {
    ///     let message = docker.create_exec("hello-world", config).await.unwrap();
    ///     docker.resize_exec(&message.id, ResizeExecOptions {
    ///         width: 80,
    ///         height: 60
    ///     });
    /// };
    /// ```
    pub async fn resize_exec(
        &self,
        exec_id: &str,
        options: ResizeExecOptions,
    ) -> Result<(), Error> {
        let url = format!("/exec/{}/resize", exec_id);

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Some(options),
            Ok(Body::empty()),
        );

        self.process_into_unit(req).await
    }
}
