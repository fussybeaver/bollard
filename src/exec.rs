//! Exec API: Run new commands inside running containers

use arrayvec::ArrayVec;
use failure::Error;
use futures::{stream, Stream};
use http::header::{CONNECTION, UPGRADE};
use http::request::Builder;
use hyper::rt::Future;
use hyper::Body;
use hyper::Method;
use serde::ser::Serialize;

use super::{Docker, DockerChain};
use either::EitherStream;

use container::LogOutput;

/// Exec configuration used in the [Create Exec API](../struct.Docker.html#method.create_exec)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateExecOptions<T>
where
    T: AsRef<str> + Serialize,
{
    /// Attach to `stdin` of the exec command.
    pub attach_stdin: Option<bool>,
    /// Attach to stdout of the exec command.
    pub attach_stdout: Option<bool>,
    /// Attach to stderr of the exec command.
    pub attach_stderr: Option<bool>,
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

/// Result type for the [Create Exec API](../struct.Docker.html#method.create_exec)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct CreateExecResults {
    pub id: String,
}

/// Exec configuration used in the [Create Exec API](../struct.Docker.html#method.create_exec)
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StartExecOptions {
    /// Detach from the command.
    pub detach: bool,
}

/// Result type for the [Start Exec API](../struct.Docker.html#method.start_exec)
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub enum StartExecResults {
    Attached { log: LogOutput },
    Detached,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(missing_docs)]
pub struct ExecProcessConfig {
    pub user: Option<String>,
    pub privileged: Option<bool>,
    pub tty: bool,
    pub entrypoint: String,
    pub arguments: Vec<String>,
}

/// Result type for the [Inspect Exec API](../struct.Docker.html#method.inspect_exec)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct ExecInspect {
    pub can_remove: bool,
    #[serde(rename = "ContainerID")]
    pub container_id: String,
    pub detach_keys: String,
    pub exit_code: Option<u64>,
    #[serde(rename = "ID")]
    pub id: String,
    pub open_stderr: bool,
    pub open_stdin: bool,
    pub open_stdout: bool,
    pub process_config: ExecProcessConfig,
    pub running: bool,
    pub pid: u64,
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
    ///  - [Create Exec Options](container/struct.CreateExecOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A [Create Exec Results](container/struct.CreateExecResults.html) struct, wrapped in a
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
    pub fn create_exec<T>(
        &self,
        container_name: &str,
        config: CreateExecOptions<T>,
    ) -> impl Future<Item = CreateExecResults, Error = Error>
    where
        T: AsRef<str> + Serialize,
    {
        let url = format!("/containers/{}/exec", container_name);

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::POST),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Docker::serialize_payload(Some(config)),
        );

        self.process_into_value(req)
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
    ///  - Container name as string slice.
    ///
    /// # Returns
    ///
    ///  - [Log Output](container/enum.LogOutput.html) enum, wrapped in a Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::exec::StartExecOptions;
    ///
    /// docker.start_exec("hello-world", None::<StartExecOptions>);
    /// ```
    pub fn start_exec(
        &self,
        container_name: &str,
        config: Option<StartExecOptions>,
    ) -> impl Stream<Item = StartExecResults, Error = Error> {
        let url = format!("/exec/{}/start", container_name);

        match config {
            Some(StartExecOptions { detach: true, .. }) => {
                let req = self.build_request::<_, String, String>(
                    &url,
                    Builder::new().method(Method::POST),
                    Ok(None::<ArrayVec<[(_, _); 0]>>),
                    Docker::serialize_payload(config),
                );

                EitherStream::A(
                    self.process_into_unit(req)
                        .map(|_| StartExecResults::Detached)
                        .into_stream(),
                )
            }
            _ => {
                let req = self.build_request::<_, String, String>(
                    &url,
                    Builder::new()
                        .method(Method::POST)
                        .header(CONNECTION, "Upgrade")
                        .header(UPGRADE, "tcp"),
                    Ok(None::<ArrayVec<[(_, _); 0]>>),
                    Docker::serialize_payload(config.or_else(|| {
                        Some(StartExecOptions {
                            ..Default::default()
                        })
                    })),
                );

                EitherStream::B(
                    self.process_upgraded_stream_string(req)
                        .map(|s| StartExecResults::Attached { log: s }),
                )
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
    ///  - Container name as string slice.
    ///
    /// # Returns
    ///
    ///  - An [ExecInspect](container/struct.ExecInspect.html) struct, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.inspect_exec("hello-world");
    /// ```
    pub fn inspect_exec(
        &self,
        container_name: &str,
    ) -> impl Future<Item = ExecInspect, Error = Error> {
        let url = format!("/exec/{}/json", container_name);

        let req = self.build_request::<_, String, String>(
            &url,
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value(req)
    }
}

impl DockerChain {
    /// ---
    ///
    /// # Create Exec
    ///
    /// Run a command inside a running container. Consumes the client instance.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///  - [Create Exec Options](container/struct.CreateExecOptions.html) struct.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a
    ///  [Create Exec Results](container/struct.CreateExecResults.html) struct, wrapped in a
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
    /// docker.chain().create_exec("hello-world", config);
    /// ```
    pub fn create_exec<T>(
        self,
        container_name: &str,
        config: CreateExecOptions<T>,
    ) -> impl Future<Item = (DockerChain, CreateExecResults), Error = Error>
    where
        T: AsRef<str> + Serialize,
    {
        self.inner
            .create_exec(container_name, config)
            .map(|result| (self, result))
    }

    /// ---
    ///
    /// # Start Exec
    ///
    /// Starts a previously set up exec instance. If detach is true, this endpoint returns
    /// immediately after starting the command. Consumes the client instance.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and a [Log
    ///  Output](container/enum.LogOutput.html) enum, wrapped in a Stream.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::exec::StartExecOptions;
    ///
    /// docker.chain().start_exec("hello-world", None::<StartExecOptions>);
    /// ```
    pub fn start_exec(
        self,
        container_name: &str,
        config: Option<StartExecOptions>,
    ) -> impl Future<
        Item = (
            DockerChain,
            impl Stream<Item = StartExecResults, Error = Error>,
        ),
        Error = Error,
    > {
        self.inner
            .start_exec(container_name, config)
            .into_future()
            .map(|(first, rest)| match first {
                Some(head) => (self, EitherStream::A(stream::once(Ok(head)).chain(rest))),
                None => (self, EitherStream::B(stream::empty())),
            })
            .map_err(|(err, _)| err)
    }

    /// ---
    ///
    /// # Inspect Exec
    ///
    /// Return low-level information about an exec instance. Consumes the client instance.
    ///
    /// # Arguments
    ///
    ///  - Container name as string slice.
    ///
    /// # Returns
    ///
    ///  - A Tuple containing the original [DockerChain](struct.Docker.html) instance, and an
    ///  [ExecInspect](container/struct.ExecInspect.html) struct, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.chain().inspect_exec("hello-world");
    /// ```
    pub fn inspect_exec(
        self,
        container_name: &str,
    ) -> impl Future<Item = (DockerChain, ExecInspect), Error = Error> {
        self.inner
            .inspect_exec(container_name)
            .map(|result| (self, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper_mock::HostToReplyConnector;
    use tokio::runtime::Runtime;

    #[test]
    fn test_start_exec() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();

        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 101 UPGRADED\r\nServer: mock1\r\nContent-Type: application/vnd.docker.raw-stream\r\nConnection: Upgrade\r\nUpgrade: tcp\r\n\r\n# Server configuration\nconfig uhttpd main".to_string()
        );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

        let options = Some(StartExecOptions { detach: false });

        let results = docker.start_exec("68099c450e6a", options);

        let future = results.into_future().map(|(result, _)| {
            assert!(match result {
                Some(StartExecResults::Attached {
                    log: LogOutput::Console { ref message },
                }) if message == "# Server configuration" => true,
                _ => false,
            })
        });

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e.0);
                Err(e.0)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }

    #[test]
    fn test_inspect_exec() {
        let mut rt = Runtime::new().unwrap();
        let mut connector = HostToReplyConnector::default();

        connector.m.insert(
            format!("{}://5f", if cfg!(windows) { "net.pipe" } else { "unix" }),
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 393\r\n\r\n{\"ID\":\"6b8cf3d95b64cf32d140836f4a3b8f03c1b895398f6fdbd33b69db06fa04d897\",\"Running\":true,\"ExitCode\":null,\"ProcessConfig\":{\"tty\":false,\"entrypoint\":\"/bin/cat\",\"arguments\":[\"/etc/config/uhttpd\"],\"privileged\":false},\"OpenStdin\":false,\"OpenStderr\":false,\"OpenStdout\":true,\"CanRemove\":false,\"ContainerID\":\"a181d0e0bf4bbf0e37d8eb1d68677e0abef838f1aa4d8757c43c1216cfdaa965\",\"DetachKeys\":\"\",\"Pid\":7169}\r\n".to_string()
        );

        let docker = Docker::connect_with_host_to_reply(connector, "_".to_string(), 5).unwrap();

        let results = docker.inspect_exec("68099c450e6a");

        let future = results.map(|result| {
            assert_eq!(
                "/etc/config/uhttpd".to_string(),
                result.process_config.arguments[0]
            )
        });

        rt.block_on(future)
            .or_else(|e| {
                println!("{:?}", e);
                Err(e)
            })
            .unwrap();

        rt.shutdown_now().wait().unwrap();
    }
}
