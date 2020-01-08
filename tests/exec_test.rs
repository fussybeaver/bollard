#![type_length_limit = "2097152"]

use bollard::container::*;
use bollard::errors::Error;
use bollard::exec::*;
use bollard::Docker;

use futures_util::stream::TryStreamExt;
use tokio::runtime::Runtime;

#[macro_use]
pub mod common;
use crate::common::*;

async fn start_exec_test(docker: Docker) -> Result<(), Error> {
    create_daemon(&docker, "integration_test_start_exec_test").await?;

    let message = &docker
        .create_exec(
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
        .await?;

    let vec = &docker
        .start_exec(&message.id, None::<StartExecOptions>)
        .try_collect::<Vec<_>>()
        .await?;

    assert!(vec.iter().take(2).any(|line| {
        println!("{:?}", line);
        let expected = if cfg!(windows) {
            "<configuration>\r"
        } else {
            "config uhttpd main"
        };
        match line {
            StartExecResults::Attached { ref log } if format!("{}", log) == expected => true,
            _ => false,
        }
    }));

    &docker
        .kill_container(
            "integration_test_start_exec_test",
            None::<KillContainerOptions<String>>,
        )
        .await?;

    &docker
        .wait_container(
            "integration_test_start_exec_test",
            None::<WaitContainerOptions<String>>,
        )
        .try_collect::<Vec<_>>()
        .await?;

    &docker
        .remove_container(
            "integration_test_start_exec_test",
            None::<RemoveContainerOptions>,
        )
        .await?;

    Ok(())
}

async fn inspect_exec_test(docker: Docker) -> Result<(), Error> {
    create_daemon(&docker, "integration_test_inspect_exec_test").await?;

    let message = &docker
        .create_exec(
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
        .await?;

    &docker
        .start_exec(&message.id, Some(StartExecOptions { detach: true }))
        .try_collect::<Vec<_>>()
        .await?;

    let exec_process = &docker.inspect_exec(&message.id).await?;

    assert_eq!(
        if cfg!(windows) {
            "cmd.exe".to_string()
        } else {
            "/bin/cat".to_string()
        },
        exec_process.process_config.entrypoint
    );

    &docker
        .kill_container(
            "integration_test_inspect_exec_test",
            None::<KillContainerOptions<String>>,
        )
        .await?;

    &docker
        .wait_container(
            "integration_test_inspect_exec_test",
            None::<WaitContainerOptions<String>>,
        )
        .try_collect::<Vec<_>>()
        .await;

    &docker
        .remove_container(
            "integration_test_inspect_exec_test",
            None::<RemoveContainerOptions>,
        )
        .await?;

    Ok(())
}

#[test]
fn integration_test_start_exec() {
    connect_to_docker_and_run!(start_exec_test);
}

#[test]
fn integration_test_inspect_exec() {
    connect_to_docker_and_run!(inspect_exec_test);
}
