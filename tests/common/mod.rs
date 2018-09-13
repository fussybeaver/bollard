extern crate failure;
extern crate futures;

use self::failure::Error;
use self::futures::future;
use hyper::client::connect::Connect;
use hyper::rt::{Future, Stream};
use tokio::runtime::Runtime;

use std::collections::HashMap;

use boondock::container::*;
use boondock::image::*;
use boondock::DockerChain;

#[allow(unused_macros)]
macro_rules! rt_exec {
    ($docker_call:expr, $assertions:expr) => {{
        let mut rt = Runtime::new().unwrap();
        let call = $docker_call;
        let res = $assertions(
            rt.block_on(call)
                .or_else(|e| {
                    println!("{}", e);
                    Err(e)
                })
                .unwrap(),
        );
        rt.shutdown_now().wait().unwrap();
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
        rt.shutdown_now().wait().unwrap();
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
        #[cfg(all(unix, not(feature = "test_http"), not(feature = "openssl")))]
        $exec(Docker::connect_with_unix_defaults().unwrap());
        #[cfg(feature = "test_http")]
        $exec(Docker::connect_with_http_defaults().unwrap());
        #[cfg(feature = "openssl")]
        $exec(Docker::connect_with_ssl_defaults().unwrap());
        #[cfg(windows)]
        $exec(Docker::connect_with_named_pipe_defaults().unwrap());
    }};
}

#[allow(dead_code)]
pub(crate) fn run_runtime<T, Y>(mut rt: Runtime, future: T)
where
    T: Future<Item = Y, Error = Error> + Send + 'static,
    Y: Send + 'static,
{
    rt.block_on(future)
        .or_else(|e| {
            println!("{}", e);
            Err(e)
        })
        .unwrap();

    rt.shutdown_now().wait().unwrap();
}

#[allow(dead_code)]
pub fn chain_create_container_hello_world<C>(
    chain: DockerChain<C>,
    container_name: &'static str,
) -> impl Future<Item = DockerChain<C>, Error = Error>
where
    C: Connect + Sync + 'static,
{
    let image = || {
        if cfg!(windows) {
            String::from("hello-world:nanoserver")
        } else {
            String::from("hello-world:linux")
        }
    };

    let cmd = if cfg!(windows) {
        vec![
            "cmd".to_string(),
            "/C".to_string(),
            "type C:\\hello.txt".to_string(),
        ]
    } else {
        vec!["/hello".to_string()]
    };

    chain
        .create_image(Some(CreateImageOptions {
            from_image: image(),
            ..Default::default()
        }))
        .and_then(move |(docker, _)| {
            docker.create_container(
                Some(CreateContainerOptions {
                    name: container_name.to_string(),
                }),
                Config {
                    cmd: cmd,
                    image: Some(image()),
                    ..Default::default()
                },
            )
        })
        .map(|(docker, result)| {
            assert_ne!(result.id.len(), 0);
            docker
        })
        .and_then(move |docker| docker.start_container(container_name, None))
        .and_then(move |(docker, _)| docker.wait_container(container_name, None))
        .map(|(docker, stream)| {
            stream
                .take(1)
                .into_future()
                .map(|(head, _)| assert_eq!(head.unwrap().status_code, 0))
                .or_else(|e| {
                    println!("{}", e.0);
                    Err(e.0)
                })
                .wait()
                .unwrap();
            docker
        })
}

#[allow(dead_code)]
pub fn chain_create_registry<C>(
    chain: DockerChain<C>,
    container_name: &'static str,
) -> impl Future<Item = DockerChain<C>, Error = Error>
where
    C: Connect + Sync + 'static,
{
    let image = || {
        if cfg!(windows) {
            String::from("stefanscherer/registry-windows")
        } else {
            String::from("registry:2")
        }
    };

    let cmd = || {
        if cfg!(windows) {
            vec![
                "\\registry.exe".to_string(),
                "serve".to_string(),
                "/config/config.yml".to_string(),
            ]
        } else {
            vec![
                "/entrypoint.sh".to_string(),
                "/etc/docker/registry/config.yml".to_string(),
            ]
        }
    };

    chain
        .create_image(Some(CreateImageOptions {
            from_image: image(),
            ..Default::default()
        }))
        .and_then(move |(docker, _)| {
            docker.create_container(
                Some(CreateContainerOptions {
                    name: container_name.to_string(),
                }),
                Config {
                    attach_stdout: Some(false),
                    attach_stderr: Some(false),
                    cmd: cmd(),
                    image: Some(image()),
                    exposed_ports: Some(
                        [("5000/tcp".to_string(), HashMap::new())]
                            .iter()
                            .cloned()
                            .collect::<HashMap<String, HashMap<(), ()>>>(),
                    ),
                    host_config: Some(HostConfig {
                        port_bindings: Some(
                            [(
                                "5000/tcp".to_string(),
                                vec![PortBinding {
                                    host_ip: ::std::env::var("HOST_IP")
                                        .unwrap_or("0.0.0.0".to_string()),
                                    host_port: "5000".to_string(),
                                }],
                            )].iter()
                                .cloned()
                                .collect::<HashMap<String, Vec<PortBinding>>>(),
                        ),
                        publish_all_ports: Some(true),
                        restart_policy: Some(RestartPolicy {
                            name: Some("always".to_string()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
        })
        .and_then(move |(docker, _)| docker.start_container(container_name, None))
        .map(|(docker, _)| docker)
}

#[allow(dead_code)]
pub fn chain_create_noop<C>(
    chain: DockerChain<C>,
) -> impl Future<Item = DockerChain<C>, Error = Error>
where
    C: Connect + Sync + 'static,
{
    future::ok(chain)
}

#[allow(dead_code)]
pub fn chain_create_daemon<C>(
    chain: DockerChain<C>,
    container_name: &'static str,
) -> impl Future<Item = DockerChain<C>, Error = Error>
where
    C: Connect + Sync + 'static,
{
    let image = || {
        if cfg!(windows) {
            String::from("stefanscherer/consul-windows")
        } else {
            String::from("fnichol/uhttpd")
        }
    };

    let cmd = || {
        if cfg!(windows) {
            vec![
                "C:\\consul.exe".to_string(),
                "agent".to_string(),
                "-ui".to_string(),
                "-dev".to_string(),
                "-client".to_string(),
                "0.0.0.0".to_string(),
            ]
        } else {
            vec![
                "/usr/sbin/run_uhttpd".to_string(),
                "-f".to_string(),
                "-p".to_string(),
                "80".to_string(),
                "-h".to_string(),
                "/www".to_string(),
            ]
        }
    };

    #[cfg(unix)]
    let chain = chain
        .create_image(Some(CreateImageOptions {
            from_image: image(),
            ..Default::default()
        }))
        .map(|(docker, _)| docker);

    chain
        .and_then(move |docker| {
            docker.create_container(
                Some(CreateContainerOptions {
                    name: container_name.to_string(),
                }),
                Config {
                    cmd: cmd(),
                    image: Some(image()),
                    ..Default::default()
                },
            )
        })
        .map(|(docker, result)| {
            assert_ne!(result.id.len(), 0);
            (docker, result)
        })
        .and_then(move |(docker, _)| docker.start_container(container_name, None))
        .map(|(docker, _)| docker)
}

#[allow(dead_code)]
pub fn chain_kill_container<C>(
    chain: DockerChain<C>,
    container_name: &'static str,
) -> impl Future<Item = DockerChain<C>, Error = Error>
where
    C: Connect + Sync + 'static,
{
    chain
        .kill_container(container_name, None)
        .and_then(move |(docker, _)| docker.wait_container(container_name, None))
        .and_then(move |(docker, _)| docker.remove_container(container_name, None))
        .map(|(docker, _)| docker)
}

#[allow(dead_code)]
pub fn chain_create_image_hello_world<C>(
    chain: DockerChain<C>,
) -> impl Future<Item = DockerChain<C>, Error = Error>
where
    C: Connect + Sync + 'static,
{
    let image = || {
        if cfg!(windows) {
            String::from("hello-world:nanoserver")
        } else {
            String::from("hello-world:latest")
        }
    };

    chain
        .create_image(Some(CreateImageOptions {
            from_image: image(),
            ..Default::default()
        }))
        .map(|(docker, result)| {
            result
                .take(1)
                .into_future()
                .map(|(head, _)| {
                    match head.unwrap() {
                        CreateImageResults::CreateImageProgressResponse {
                            id: Some(ref id),
                            ..
                        } => assert_eq!(id, "latest"),
                        _ => panic!(),
                    };
                })
                .or_else(|e| {
                    println!("{}", e.0);
                    Err(e.0)
                })
                .wait()
                .unwrap();
            docker
        })
}
