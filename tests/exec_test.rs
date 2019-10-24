#![type_length_limit = "2097152"]
extern crate bollard;
extern crate hyper;
extern crate tokio;

use bollard::container::*;
use bollard::exec::*;
use bollard::Docker;

use hyper::rt::{Future, Stream};
use tokio::runtime::Runtime;

#[macro_use]
pub mod common;
use crate::common::*;

fn start_exec_test(docker: Docker) {
    let rt = Runtime::new().unwrap();
    let future = chain_create_daemon(docker.chain(), "integration_test_start_exec_test")
        .and_then(move |docker| {
            docker.create_exec(
                "integration_test_start_exec_test",
                CreateExecOptions {
                    attach_stdout: Some(true),
                    cmd: if cfg!(windows) {
                        Some(vec![
                            "cmd.exe",
                            "/C",
                            "type",
                            "C:\\Windows\\System32\\Inetsrv\\Config\\ApplicationHost.config",
                        ])
                    } else {
                        Some(vec!["/bin/cat", "/etc/config/uhttpd"])
                    },
                    ..Default::default()
                },
            )
        })
        .and_then(|(docker, message)| docker.start_exec(&message.id, None::<StartExecOptions>))
        .and_then(|(docker, stream)| stream.take(2).collect().map(|v| (docker, v)))
        .map(|(docker, lst)| {
            assert!(lst.into_iter().any(|line| {
                println!("{:?}", line);
                let expected = if cfg!(windows) {
                    "<configuration>\r"
                } else {
                    "config uhttpd main"
                };
                match line {
                    StartExecResults::Attached { ref log } if format!("{}", log) == expected => {
                        true
                    }
                    _ => false,
                }
            }));
            docker
        })
        .and_then(|docker| {
            docker.kill_container(
                "integration_test_start_exec_test",
                None::<KillContainerOptions<String>>,
            )
        })
        .and_then(|(docker, _)| {
            docker.wait_container(
                "integration_test_start_exec_test",
                None::<WaitContainerOptions<String>>,
            )
        })
        .and_then(|(docker, _)| {
            docker.remove_container(
                "integration_test_start_exec_test",
                None::<RemoveContainerOptions>,
            )
        });

    run_runtime(rt, future);
}

fn inspect_exec_test(docker: Docker) {
    let rt = Runtime::new().unwrap();
    let future = chain_create_daemon(docker.chain(), "integration_test_inspect_exec_test")
        .and_then(move |docker| {
            docker.create_exec(
                "integration_test_inspect_exec_test",
                CreateExecOptions {
                    attach_stdout: Some(true),
                    cmd: if cfg!(windows) {
                        Some(vec![
                            "cmd.exe",
                            "/C",
                            "type",
                            "C:\\Windows\\System32\\Inetsrv\\Config\\ApplicationHost.config",
                        ])
                    } else {
                        Some(vec!["/bin/cat", "/etc/config/uhttpd"])
                    },
                    ..Default::default()
                },
            )
        })
        .and_then(|(docker, message)| {
            docker
                .start_exec(&message.id, Some(StartExecOptions { detach: true }))
                .map(|(docker, _)| (docker, message.id))
        })
        .and_then(|(docker, id)| docker.inspect_exec(&id))
        .map(|(docker, exec_process)| {
            assert_eq!(
                if cfg!(windows) {
                    "cmd.exe".to_string()
                } else {
                    "/bin/cat".to_string()
                },
                exec_process.process_config.entrypoint
            );
            docker
        })
        .and_then(|docker| {
            docker.kill_container(
                "integration_test_inspect_exec_test",
                None::<KillContainerOptions<String>>,
            )
        })
        .and_then(|(docker, _)| {
            docker.wait_container(
                "integration_test_inspect_exec_test",
                None::<WaitContainerOptions<String>>,
            )
        })
        .and_then(|(docker, _)| {
            docker.remove_container(
                "integration_test_inspect_exec_test",
                None::<RemoveContainerOptions>,
            )
        });

    run_runtime(rt, future);
}

#[test]
fn integration_test_start_exec() {
    connect_to_docker_and_run!(start_exec_test);
}

#[test]
fn integration_test_inspect_exec() {
    connect_to_docker_and_run!(inspect_exec_test);
}
