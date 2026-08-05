use bytes::Bytes;
use futures_core::Stream;
use futures_util::stream::TryStreamExt;
use std::future::Future;
use tokio::runtime::Runtime;

use bollard::auth::DockerCredentials;
use bollard::errors::Error;
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{CreateContainerOptionsBuilder, CreateImageOptionsBuilder};
use bollard::Docker;

#[allow(unused_macros)]
macro_rules! rt_exec {
    ($docker_call:expr, $assertions:expr) => {{
        let rt = Runtime::new().unwrap();
        let res = $assertions(rt.block_on($docker_call).unwrap());
        res
    }};
}

#[allow(unused_macros)]
macro_rules! connect_to_docker_and_run {
    ($exec:expr) => {{
        let rt = Runtime::new().unwrap();
        #[cfg(all(unix, not(feature = "test_http"), not(feature = "test_ssl")))]
        let docker = Docker::connect_with_unix_defaults().unwrap();
        #[cfg(all(feature = "test_http", not(feature = "test_ssl")))]
        let docker = Docker::connect_with_http_defaults().unwrap();
        #[cfg(feature = "test_ssl")]
        let docker = Docker::connect_with_ssl_defaults().unwrap();
        #[cfg(windows)]
        let docker = Docker::connect_with_named_pipe_defaults().unwrap();
        let fut = async move { $exec(docker.negotiate_version().await.unwrap()).await };
        run_runtime(rt, fut);
    }};
}

pub fn integration_test_registry_credentials() -> DockerCredentials {
    DockerCredentials {
        username: Some("bollard".to_string()),
        password: std::env::var("REGISTRY_PASSWORD").ok(),
        ..Default::default()
    }
}

pub(crate) fn registry_http_addr() -> String {
    if ::std::env::var("DISABLE_REGISTRY").is_ok() {
        String::new()
    } else {
        format!(
            "{}/",
            ::std::env::var("REGISTRY_HTTP_ADDR").unwrap_or_else(|_| "localhost:5000".to_string())
        )
    }
}

#[allow(dead_code)]
pub(crate) fn run_runtime<T>(rt: Runtime, future: T)
where
    T: Future<Output = Result<(), Error>>,
{
    rt.block_on(future)
        .map_err(|e| {
            println!("{e:?}");
            e
        })
        .unwrap();
}

#[allow(dead_code)]
pub async fn create_container_hello_world(
    docker: &Docker,
    container_name: &'static str,
) -> Result<String, Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    let cmd = if cfg!(windows) {
        Some(vec![
            "cmd".to_string(),
            "/C".to_string(),
            "type C:\\hello.txt".to_string(),
        ])
    } else {
        Some(vec!["/hello".to_string()])
    };

    let _ = &docker
        .create_image(
            Some(
                CreateImageOptionsBuilder::default()
                    .from_image(&image)
                    .build(),
            ),
            None,
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .try_collect::<Vec<_>>()
        .await?;

    let result = &docker
        .create_container(
            Some(
                CreateContainerOptionsBuilder::default()
                    .name(container_name)
                    .build(),
            ),
            ContainerCreateBody {
                cmd,
                image: Some(image.clone()),
                ..Default::default()
            },
        )
        .await?;

    assert_ne!(result.id.len(), 0);

    let _ = &docker.start_container(container_name, None).await?;

    let wait = &docker
        .wait_container(container_name, None)
        .try_collect::<Vec<_>>()
        .await?;

    assert_eq!(wait.first().unwrap().status_code, 0);
    Ok(image)
}

#[allow(dead_code)]
pub async fn create_shell_daemon(
    docker: &Docker,
    container_name: &'static str,
) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}nanoserver/iis", registry_http_addr())
    } else {
        format!("{}alpine", registry_http_addr())
    };

    let _ = &docker
        .create_image(
            Some(
                CreateImageOptionsBuilder::default()
                    .from_image(&image)
                    .build(),
            ),
            None,
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .try_collect::<Vec<_>>()
        .await?;

    let result = &docker
        .create_container(
            Some(
                CreateContainerOptionsBuilder::default()
                    .name(container_name)
                    .build(),
            ),
            ContainerCreateBody {
                image: Some(image),
                open_stdin: Some(true),
                ..Default::default()
            },
        )
        .await?;

    assert_ne!(result.id.len(), 0);

    let _ = &docker.start_container(container_name, None).await?;

    Ok(())
}

#[allow(dead_code)]
pub async fn create_daemon(docker: &Docker, container_name: &'static str) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}nanoserver/iis", registry_http_addr())
    } else {
        format!("{}fussybeaver/uhttpd", registry_http_addr())
    };

    let cmd = if cfg!(windows) {
        Some(vec![
            "net".to_string(),
            "start".to_string(),
            "w3svc".to_string(),
        ])
    } else {
        Some(vec![
            "/usr/sbin/run_uhttpd".to_string(),
            "-f".to_string(),
            "-p".to_string(),
            "80".to_string(),
            "-h".to_string(),
            "/www".to_string(),
        ])
    };

    let _ = &docker
        .create_image(
            Some(
                CreateImageOptionsBuilder::default()
                    .from_image(&image)
                    .build(),
            ),
            None,
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .try_collect::<Vec<_>>()
        .await?;

    let result = &docker
        .create_container(
            Some(
                CreateContainerOptionsBuilder::default()
                    .name(container_name)
                    .build(),
            ),
            ContainerCreateBody {
                cmd,
                image: Some(image),
                ..Default::default()
            },
        )
        .await?;

    assert_ne!(result.id.len(), 0);

    let _ = &docker.start_container(container_name, None).await?;

    Ok(())
}

#[allow(dead_code)]
pub async fn kill_container(docker: &Docker, container_name: &'static str) -> Result<(), Error> {
    let _ = &docker.kill_container(container_name, None).await?;

    let _ = &docker
        .wait_container(container_name, None)
        .try_collect::<Vec<_>>()
        .await;

    let _ = &docker.remove_container(container_name, None).await?;

    Ok(())
}

#[allow(dead_code)]
pub async fn create_image_hello_world(docker: &Docker) -> Result<String, Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    let result = &docker
        .create_image(
            Some(
                CreateImageOptionsBuilder::default()
                    .from_image(&image)
                    .build(),
            ),
            None,
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .try_collect::<Vec<_>>()
        .await?;

    assert_eq!(
        result.first().unwrap().id.as_ref().unwrap(),
        if cfg!(windows) { "nanoserver" } else { "linux" }
    );

    Ok(image)
}

#[allow(dead_code)]
pub async fn concat_byte_stream<S>(s: S) -> Result<Vec<u8>, Error>
where
    S: Stream<Item = Result<Bytes, Error>>,
{
    s.try_fold(Vec::new(), |mut acc, chunk| async move {
        acc.extend_from_slice(&chunk[..]);
        Ok(acc)
    })
    .await
}

#[cfg(feature = "buildkit")]
pub mod buildkit_test {
    //! Shared provenance and builder helpers for BuildKit integration tests.
    #![allow(dead_code)]
    // This module is compiled by every BuildKit integration test binary, although
    // only the LLB compatibility suite currently consumes these helpers.

    use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
    use bollard::grpc::driver::docker_container::{DockerContainer, DockerContainerBuilder};
    use bollard::models::{ContainerInspectResponse, ExecInspectResponse, ImageInspect};
    use bollard::query_parameters::InspectContainerOptions;
    use bollard::Docker;
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};

    use super::Error;

    const GO_MOD_CONTENT: &str = include_str!("../../codegen/llb-parity/go.mod");
    const OPS_PROTO_BYTES: &[u8] = include_bytes!("../../codegen/proto/resources/pb/ops.proto");

    /// Provenance record for one BuildKit integration run.
    #[derive(Debug, Clone, Default)]
    pub struct BuildkitVersionRecord {
        pub requested_image: String,
        pub resolved_image_id: String,
        pub resolved_repo_digests: Vec<String>,
        pub buildkitd_version: String,
        pub buildctl_version: String,
        pub go_oracle_version: String,
        pub ops_proto_sha256: String,
    }

    impl std::fmt::Display for BuildkitVersionRecord {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            writeln!(f, "=== Bollard BuildKit compatibility baseline ===")?;
            writeln!(f, "requested_image: {}", self.requested_image)?;
            writeln!(f, "resolved_image_id: {}", self.resolved_image_id)?;
            writeln!(f, "resolved_repo_digests: {:?}", self.resolved_repo_digests)?;
            writeln!(f, "buildkitd_version: {}", self.buildkitd_version)?;
            writeln!(f, "buildctl_version: {}", self.buildctl_version)?;
            writeln!(f, "go_oracle_version: {}", self.go_oracle_version)?;
            writeln!(f, "ops_proto_sha256: {}", self.ops_proto_sha256)?;
            write!(f, "===")
        }
    }

    /// Read the optional image override used by compatibility CI.
    fn test_image() -> Option<String> {
        std::env::var("BOLLARD_BUILDKIT_TEST_IMAGE")
            .ok()
            .filter(|image| !image.is_empty())
    }

    /// Construct a BuildKit builder using the configured test image.
    pub fn builder(docker: &Docker) -> DockerContainerBuilder {
        let mut builder = DockerContainerBuilder::new(docker);
        if let Some(image) = test_image() {
            builder.image(&image);
        }
        builder
    }

    /// Parse the direct BuildKit requirement from the pinned Go module.
    fn parse_go_buildkit_version(go_mod: &str) -> Option<String> {
        for line in go_mod.lines() {
            if !line.contains("github.com/moby/buildkit") || line.contains("// indirect") {
                continue;
            }
            for token in line.split_whitespace() {
                if token.len() > 1 && token.starts_with('v') && token.as_bytes()[1].is_ascii_digit()
                {
                    return Some(token.to_string());
                }
            }
        }
        None
    }

    fn go_oracle_version() -> String {
        parse_go_buildkit_version(GO_MOD_CONTENT).unwrap_or_else(|| String::from("unknown"))
    }

    fn ops_proto_sha256() -> String {
        hex::encode(Sha256::digest(OPS_PROTO_BYTES))
    }

    /// Capture image, daemon, oracle, and schema identities after bootstrap.
    pub async fn record_version(
        docker: &Docker,
        container: &DockerContainer,
    ) -> Result<BuildkitVersionRecord, Error> {
        let name = container.name();
        let inspect: ContainerInspectResponse = docker
            .inspect_container(name, None::<InspectContainerOptions>)
            .await?;
        let requested_image = inspect
            .config
            .and_then(|config| config.image)
            .unwrap_or_default();

        let image_inspect: ImageInspect = docker.inspect_image(&requested_image).await?;
        let resolved_image_id = image_inspect.id.unwrap_or_default();
        let resolved_repo_digests = image_inspect.repo_digests.unwrap_or_default();
        let buildkitd_version = exec_command(docker, name, vec!["buildkitd", "--version"]).await?;
        let buildctl_version = exec_command(docker, name, vec!["buildctl", "--version"]).await?;

        if resolved_repo_digests.is_empty() {
            return Err(Error::IOError {
                err: std::io::Error::other("BuildKit image has no repository digest"),
            });
        }
        if buildkitd_version.is_empty() || buildctl_version.is_empty() {
            return Err(Error::IOError {
                err: std::io::Error::other("BuildKit version probe returned no output"),
            });
        }

        Ok(BuildkitVersionRecord {
            requested_image,
            resolved_image_id,
            resolved_repo_digests,
            buildkitd_version,
            buildctl_version,
            go_oracle_version: go_oracle_version(),
            ops_proto_sha256: ops_proto_sha256(),
        })
    }

    async fn exec_command(
        docker: &Docker,
        container_name: &str,
        command: Vec<&str>,
    ) -> Result<String, Error> {
        let exec_id = docker
            .create_exec(
                container_name,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(command.clone()),
                    ..Default::default()
                },
            )
            .await?
            .id;
        let mut output = Vec::new();
        if let StartExecResults::Attached {
            output: mut stream, ..
        } = docker
            .start_exec(&exec_id, None::<StartExecOptions>)
            .await?
        {
            while let Some(Ok(log)) = stream.next().await {
                output.extend_from_slice(log.into_bytes().as_ref());
            }
        }

        let output = String::from_utf8_lossy(&output).trim().to_string();
        let inspect: ExecInspectResponse = docker.inspect_exec(&exec_id).await?;
        if inspect.exit_code != Some(0) {
            return Err(Error::DockerContainerWaitError {
                error: format!(
                    "command {command:?} failed with exit code {:?}: {output}",
                    inspect.exit_code
                ),
                code: inspect.exit_code.unwrap_or(-1),
            });
        }
        Ok(output)
    }
}
