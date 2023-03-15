use bytes::Bytes;
use futures_core::Stream;
use futures_util::stream::TryStreamExt;
use std::future::Future;
use tokio::runtime::Runtime;

use bollard_next::auth::DockerCredentials;
use bollard_next::container::*;
use bollard_next::errors::Error;
use bollard_next::image::*;
use bollard_next::Docker;

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
        let fut = $exec(Docker::connect_with_unix_defaults().unwrap());
        #[cfg(feature = "test_http")]
        let fut = $exec(Docker::connect_with_http_defaults().unwrap());
        #[cfg(feature = "test_ssl")]
        let fut = $exec(Docker::connect_with_ssl_defaults().unwrap());
        #[cfg(windows)]
        let fut = $exec(Docker::connect_with_named_pipe_defaults().unwrap());
        run_runtime(rt, fut);
    }};
}

pub fn integration_test_registry_credentials() -> DockerCredentials {
    DockerCredentials {
        username: Some("bollard_next".to_string()),
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
            println!("{:?}", e);
            e
        })
        .unwrap();
}

#[allow(dead_code)]
pub async fn create_container_hello_world(
    docker: &Docker,
    container_name: &'static str,
) -> Result<(), Error> {
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
            Some(CreateImageOptions {
                from_image: &image[..],
                ..Default::default()
            }),
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
            Some(CreateContainerOptions {
                name: container_name.to_string(),
                platform: None,
            }),
            Config {
                cmd,
                image: Some(image),
                ..Default::default()
            },
        )
        .await?;

    assert_ne!(result.id.len(), 0);

    let _ = &docker
        .start_container(container_name, None::<StartContainerOptions<String>>)
        .await?;

    let wait = &docker
        .wait_container(container_name, None::<WaitContainerOptions<String>>)
        .try_collect::<Vec<_>>()
        .await?;

    assert_eq!(wait.first().unwrap().status_code, 0);

    Ok(())
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
            Some(CreateImageOptions {
                from_image: &image[..],
                ..Default::default()
            }),
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
            Some(CreateContainerOptions {
                name: container_name,
                platform: None,
            }),
            Config {
                image: Some(image),
                open_stdin: Some(true),
                ..Default::default()
            },
        )
        .await?;

    assert_ne!(result.id.len(), 0);

    let _ = &docker
        .start_container(container_name, None::<StartContainerOptions<String>>)
        .await?;

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
            Some(CreateImageOptions {
                from_image: &image[..],
                ..Default::default()
            }),
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
            Some(CreateContainerOptions {
                name: container_name,
                platform: None,
            }),
            Config {
                cmd,
                image: Some(image),
                ..Default::default()
            },
        )
        .await?;

    assert_ne!(result.id.len(), 0);

    let _ = &docker
        .start_container(container_name, None::<StartContainerOptions<String>>)
        .await?;

    Ok(())
}

#[allow(dead_code)]
pub async fn kill_container(docker: &Docker, container_name: &'static str) -> Result<(), Error> {
    let _ = &docker
        .kill_container(container_name, None::<KillContainerOptions<String>>)
        .await?;

    let _ = &docker
        .wait_container(container_name, None::<WaitContainerOptions<String>>)
        .try_collect::<Vec<_>>()
        .await;

    let _ = &docker.remove_container(container_name, None).await?;

    Ok(())
}

#[allow(dead_code)]
pub async fn create_image_hello_world(docker: &Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    let result = &docker
        .create_image(
            Some(CreateImageOptions {
                from_image: &image[..],
                ..Default::default()
            }),
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
        result.get(0).unwrap().id.as_ref().unwrap(),
        if cfg!(windows) { "nanoserver" } else { "linux" }
    );

    Ok(())
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
