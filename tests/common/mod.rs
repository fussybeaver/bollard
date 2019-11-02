use futures_util::try_stream::TryStreamExt;
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
        rt.shutdown_now();
        res
    }};
}

#[allow(unused_macros)]
macro_rules! rt_stream {
    ($docker_call:expr, $assertions:expr) => {{
        let mut rt = Runtime::new().unwrap();
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
        let mut rt = Runtime::new().unwrap();
        let call = $docker_call;
        $assertions(rt.block_on(call).unwrap_or_else(|_| ()));
        rt.shutdown_now().wait().unwrap();
    }};
}

#[allow(unused_macros)]
macro_rules! connect_to_docker_and_run {
    ($exec:expr) => {{
        let rt = Runtime::new().unwrap();
        #[cfg(all(unix, not(feature = "test_http"), not(feature = "openssl")))]
        let fut = $exec(Docker::connect_with_unix_defaults().unwrap());
        #[cfg(feature = "test_http")]
        let fut = $exec(Docker::connect_with_http_defaults().unwrap());
        #[cfg(feature = "openssl")]
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
                cmd: cmd,
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

// #[allow(dead_code)]
// pub fn create_registry(
//     docker: Docker,
//     container_name: &'static str,
// ) -> impl Future<Item = DockerChain, Error = Error> {
//     let image = || {
//         if cfg!(windows) {
//             String::from("stefanscherer/registry-windows")
//         } else {
//             String::from("registry:2")
//         }
//     };
//
//     let cmd = || {
//         if cfg!(windows) {
//             Some(vec![
//                 "\\registry.exe".to_string(),
//                 "serve".to_string(),
//                 "/config/config.yml".to_string(),
//             ])
//         } else {
//             Some(vec![
//                 "/entrypoint.sh".to_string(),
//                 "/etc/docker/registry/config.yml".to_string(),
//             ])
//         }
//     };
//
//     chain
//         .create_image(
//             Some(CreateImageOptions {
//                 from_image: image(),
//                 ..Default::default()
//             }),
//             if cfg!(windows) {
//                 None
//             } else {
//                 Some(integration_test_registry_credentials())
//             },
//         )
//         .and_then(move |(docker, _)| {
//             docker.create_container(
//                 Some(CreateContainerOptions {
//                     name: container_name.to_string(),
//                 }),
//                 Config {
//                     attach_stdout: Some(false),
//                     attach_stderr: Some(false),
//                     cmd: cmd(),
//                     image: Some(image()),
//                     exposed_ports: Some(
//                         [("5000/tcp".to_string(), HashMap::new())]
//                             .iter()
//                             .cloned()
//                             .collect::<HashMap<String, HashMap<(), ()>>>(),
//                     ),
//                     host_config: Some(HostConfig {
//                         port_bindings: Some(
//                             [(
//                                 "5000/tcp".to_string(),
//                                 vec![PortBinding {
//                                     host_ip: ::std::env::var("HOST_IP")
//                                         .unwrap_or("0.0.0.0".to_string()),
//                                     host_port: "5000".to_string(),
//                                 }],
//                             )]
//                             .iter()
//                             .cloned()
//                             .collect::<HashMap<String, Vec<PortBinding<String>>>>(),
//                         ),
//                         publish_all_ports: Some(true),
//                         restart_policy: Some(RestartPolicy {
//                             name: Some("always".to_string()),
//                             ..Default::default()
//                         }),
//                         ..Default::default()
//                     }),
//                     ..Default::default()
//                 },
//             )
//         })
//         .and_then(move |(docker, _)| {
//             docker.start_container(container_name, None::<StartContainerOptions<String>>)
//         })
//         .map(|(docker, _)| docker)
// }
//
// #[allow(dead_code)]
// pub fn chain_create_noop(docker: Docker) -> impl Future<Output = Result<Docker, Error>> {
//     future::ok(docker)
// }
//
#[allow(dead_code)]
pub async fn create_daemon(docker: &Docker, container_name: &'static str) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}nanoserver/iis", registry_http_addr())
    } else {
        format!("{}fnichol/uhttpd", registry_http_addr())
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

    //let docker = future::ok(docker);
    //#[cfg(unix)]

    &docker
        .create_image(
            Some(CreateImageOptions {
                from_image: &image[..],
                ..Default::default()
            }),
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
                cmd: cmd,
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

    &docker
        .remove_container(container_name, None::<RemoveContainerOptions>)
        .await?;

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
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .try_collect::<Vec<_>>()
        .await?;

    match result.get(0).unwrap() {
        CreateImageResults::CreateImageProgressResponse {
            id: Some(ref id), ..
        } => assert_eq!(id, if cfg!(windows) { "nanoserver" } else { "linux" }),
        _ => panic!(),
    };

    Ok(())
}
