use bytes::Bytes;
use futures_core::Stream;
use futures_util::stream::TryStreamExt;
use std::future::Future;
use tokio::runtime::Runtime;

use std;

use bollard::auth::DockerCredentials;
use bollard::container::*;
use bollard::errors::Error;
use bollard::image::*;
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
macro_rules! rt_stream {
    ($docker_call:expr, $assertions:expr) => {{
        let rt = Runtime::new().unwrap();
        let call = $docker_call.fold(vec![], |mut v, line| {
            v.push(line);
            future::ok::<_, Error>(v)
        });
        $assertions(
            rt.block_on(call)
                .or_else(|e| {
                    println!("{}", e);
                    Err(e)
                })
                .unwrap(),
        );
        rt.shutdown_now();
    }};
}

#[allow(unused_macros)]
macro_rules! rt_exec_ignore_error {
    ($docker_call:expr, $assertions:expr) => {{
        let rt = Runtime::new().unwrap();
        let call = $docker_call;
        $assertions(rt.block_on(call).unwrap_or_else(|_| ()));
        rt.shutdown_now().wait().unwrap();
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
        .or_else(|e| {
            println!("{:?}", e);
            Err(e)
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

    &docker
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
            }),
            Config {
                cmd,
                image: Some(image),
                ..Default::default()
            },
        )
        .await?;

    assert_ne!(result.id.len(), 0);

    &docker
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

    &docker
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
            }),
            Config {
                cmd,
                image: Some(image),
                ..Default::default()
            },
        )
        .await?;

    assert_ne!(result.id.len(), 0);

    &docker
        .start_container(container_name, None::<StartContainerOptions<String>>)
        .await?;

    Ok(())
}

#[allow(dead_code)]
pub async fn kill_container(docker: &Docker, container_name: &'static str) -> Result<(), Error> {
    &docker
        .kill_container(container_name, None::<KillContainerOptions<String>>)
        .await?;

    &docker
        .wait_container(container_name, None::<WaitContainerOptions<String>>)
        .try_collect::<Vec<_>>()
        .await?;

    &docker.remove_container(container_name, None).await?;

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
