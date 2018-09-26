extern crate boondock;
extern crate failure;
extern crate futures;
extern crate hyper;
extern crate tokio;

use boondock::container::*;
use boondock::image::*;
use boondock::Docker;

use failure::Error;
use futures::future;
use hyper::client::connect::Connect;
use hyper::rt::{Future, Stream};
use tokio::runtime::Runtime;

use std::collections::HashMap;

#[macro_use]
mod common;

/*
fn create_container_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    rt_stream!(
        docker.create_image(Some(CreateImageOptions {
            from_image: String::from("hello-world"),
            ..Default::default()
        })),
        |_| ()
    );

    rt_exec_ignore_error!(
        docker.remove_container("integration_test_create_container", None),
        |_| ()
    );

    rt_exec!(
        {
            docker.create_container(
                Some(CreateContainerOptions {
                    name: "integration_test_create_container".to_string(),
                }),
                Config {
                    cmd: vec!["/hello".to_string()],
                    image: Some("hello-world".to_string()),
                    ..Default::default()
                },
            )
        },
        |result: CreateContainerResults| assert_ne!(result.id.len(), 0)
    );

    rt_exec!(
        docker.start_container("integration_test_create_container", None),
        |_| ()
    );

    rt_stream!(
        docker.wait_container("integration_test_create_container", None),
        |result: Vec<WaitContainerResults>| assert_eq!(result[0].status_code, 0)
    );

    rt_exec!(
        docker.remove_container("integration_test_create_container", None),
        |_| ()
    );
}

fn image_push_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    rt_stream!(
        docker.create_image(Some(CreateImageOptions {
            from_image: String::from("registry:2"),
            ..Default::default()
        })),
        |_| ()
    );

    rt_exec!(
        docker.stop_container("integration_test_image_push", None),
        |_| ()
    );

    rt_exec!(
        docker.remove_container("integration_test_image_push", None),
        |_| ()
    );

    rt_exec!(
        {
            docker.create_container(
                Some(CreateContainerOptions {
                    name: "integration_test_image_push".to_string(),
                }),
                Config {
                    attach_stdout: Some(false),
                    attach_stderr: Some(false),
                    cmd: vec![
                        "/entrypoint.sh".to_string(),
                        "/etc/docker/registry/config.yml".to_string(),
                    ],
                    image: Some("registry:2".to_string()),
                    exposed_ports: Some(
                        [("5000/tcp".to_string(), HashMap::new())]
                            .iter()
                            .cloned()
                            .collect::<HashMap<String, HashMap<(), ()>>>(),
                    ),
                    host_config: Some(HostConfig {
                        port_bindings: Some(
                            [(
                                "5000".to_string(),
                                vec![PortBinding {
                                    host_ip: "127.0.0.1".to_string(),
                                    host_port: "5000".to_string(),
                                }],
                            )]
                                .iter()
                                .cloned()
                                .collect::<HashMap<String, Vec<PortBinding>>>(),
                        ),
                        //publish_all_ports: Some(true),
                        restart_policy: Some(RestartPolicy {
                            name: Some("always".to_string()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
        },
        |result: CreateContainerResults| assert_ne!(result.id.len(), 0)
    );

    rt_exec!(
        docker.start_container("integration_test_image_push", None),
        |_| ()
    );

    rt_stream!(
        docker.create_image(Some(CreateImageOptions {
            from_image: String::from("hello-world"),
            ..Default::default()
        })),
        |_| ()
    );

    rt_exec!(
        docker.tag_image(
            "hello-world",
            Some(TagImageOptions {
                repo: "localhost:5000/my-hello-world".to_string(),
                ..Default::default()
            })
        ),
        |_| ()
    );

    rt_exec!(
        docker.push_image("localhost:5000/my-hello-world", None),
        |_| ()
    );

    // clean-up

    rt_exec!(docker.remove_image("hello-world", None), |_| ());
    rt_exec!(
        docker.remove_image("localhost:5000/my-hello-world", None),
        |_| ()
    );
    rt_exec!(
        docker.stop_container("integration_test_image_push", None),
        |_| ()
    );

    rt_exec!(
        docker.remove_container("integration_test_image_push", None),
        |_| ()
    );
}
#[test]
fn integration_test_create_container() {
    connect_to_docker_and_run!(create_container_test);
}

#[test]
fn integration_test_image_push() {
    connect_to_docker_and_run!(image_push_test);
}
*/
