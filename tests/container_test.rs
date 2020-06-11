#![type_length_limit = "2097152"]

use bollard::container::*;
use bollard::errors::Error;
use bollard::image::*;
use bollard::Docker;

use futures_util::stream::TryStreamExt;
use tokio::runtime::Runtime;

use std::io::Write;

#[macro_use]
pub mod common;
use crate::common::*;

async fn list_containers_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    create_container_hello_world(&docker, "integration_test_list_containers").await?;
    let result = &docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await?;

    assert_ne!(0, result.len());
    assert!(result.iter().any(|container| &container.image == &image));

    &docker
        .remove_container(
            "integration_test_list_containers",
            None::<RemoveContainerOptions>,
        )
        .await?;

    Ok(())
}

async fn image_push_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
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

    &docker
        .tag_image(
            &image,
            Some(TagImageOptions {
                repo: format!("{}my-hello-world", registry_http_addr()),
                ..Default::default()
            }),
        )
        .await?;

    &docker
        .push_image(
            format!("{}my-hello-world", registry_http_addr()).as_ref(),
            None::<PushImageOptions<String>>,
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .await?;

    Ok(())
}

async fn container_restart_test(docker: Docker) -> Result<(), Error> {
    create_daemon(&docker, "integration_test_restart_container").await?;

    let result = &docker
        .inspect_container(
            "integration_test_restart_container",
            None::<InspectContainerOptions>,
        )
        .await?;

    let started_at = result.state.started_at;

    &docker
        .restart_container(
            "integration_test_restart_container",
            None::<RestartContainerOptions>,
        )
        .await?;
    let result = &docker
        .inspect_container(
            "integration_test_restart_container",
            None::<InspectContainerOptions>,
        )
        .await?;

    assert_ne!(started_at, result.state.started_at);
    kill_container(&docker, "integration_test_restart_container").await?;

    Ok(())
}

async fn top_processes_test(docker: Docker) -> Result<(), Error> {
    let top_options = if cfg!(windows) {
        None
    } else {
        Some(TopOptions { ps_args: "aux" })
    };

    let expected = if cfg!(windows) {
        "Name"
    } else if cfg!(feature = "test_http") {
        "PID"
    } else if cfg!(feature = "openssl") {
        "PID"
    } else if cfg!(target_os = "macos") {
        "PID"
    } else {
        "USER"
    };

    create_daemon(&docker, "integration_test_top_processes").await?;

    let result = &docker
        .top_processes("integration_test_top_processes", top_options)
        .await?;

    assert_eq!(result.titles[0], expected);
    kill_container(&docker, "integration_test_top_processes").await?;

    Ok(())
}

async fn logs_test(docker: Docker) -> Result<(), Error> {
    create_container_hello_world(&docker, "integration_test_logs").await?;

    // for some reason on windows, even though we start the container,
    // wait until it finishes, the API request for logs seems to be flaky
    // on the first request.
    #[cfg(windows)]
    &docker
        .logs(
            "integration_test_logs",
            Some(LogsOptions {
                follow: true,
                stdout: true,
                stderr: false,
                tail: "all".to_string(),
                ..Default::default()
            }),
        )
        .try_collect::<Vec<_>>()
        .await?;

    let vec = &docker
        .logs(
            "integration_test_logs",
            Some(LogsOptions {
                follow: true,
                stdout: true,
                stderr: false,
                tail: "all".to_string(),
                ..Default::default()
            }),
        )
        .try_collect::<Vec<_>>()
        .await?;

    let value = vec.get(1).unwrap();

    assert_eq!(format!("{}", value), "Hello from Docker!".to_string());

    &docker
        .remove_container("integration_test_logs", None::<RemoveContainerOptions>)
        .await?;

    Ok(())
}

async fn container_changes_test(docker: Docker) -> Result<(), Error> {
    create_container_hello_world(&docker, "integration_test_container_changes").await?;

    let result = &docker
        .container_changes("integration_test_container_changes")
        .await?;

    if cfg!(windows) {
        assert_ne!(result.as_ref().unwrap().len(), 0)
    } else {
        assert!(result.is_none())
    };

    &docker
        .remove_container(
            "integration_test_container_changes",
            None::<RemoveContainerOptions>,
        )
        .await?;

    Ok(())
}

async fn stats_test(docker: Docker) -> Result<(), Error> {
    create_daemon(&docker, "integration_test_stats").await?;

    let vec = &docker
        .stats(
            "integration_test_stats",
            Some(StatsOptions { stream: false }),
        )
        .try_collect::<Vec<_>>()
        .await?;

    let value = vec.get(0);

    assert_eq!(value.unwrap().name, "/integration_test_stats".to_string());
    kill_container(&docker, "integration_test_stats")
        .await
        .unwrap_or(());

    Ok(())
}

async fn kill_container_test(docker: Docker) -> Result<(), Error> {
    let kill_options = Some(KillContainerOptions { signal: "SIGKILL" });

    create_daemon(&docker, "integration_test_kill_container").await?;

    &docker
        .kill_container("integration_test_kill_container", kill_options)
        .await?;
    &docker
        .remove_container(
            "integration_test_kill_container",
            None::<RemoveContainerOptions>,
        )
        .await?;

    Ok(())
}

async fn update_container_test(docker: Docker) -> Result<(), Error> {
    let update_options = UpdateContainerOptions {
        memory: Some(314572800),
        memory_swap: Some(314572800),
        ..Default::default()
    };

    create_daemon(&docker, "integration_test_update_container").await?;
    &docker
        .update_container("integration_test_update_container", update_options)
        .await?;
    let result = &docker
        .inspect_container(
            "integration_test_update_container",
            None::<InspectContainerOptions>,
        )
        .await?;

    assert_eq!(314572800, result.host_config.memory.unwrap());

    &docker
        .kill_container(
            "integration_test_update_container",
            None::<KillContainerOptions<String>>,
        )
        .await?;

    &docker
        .wait_container(
            "integration_test_update_container",
            None::<WaitContainerOptions<String>>,
        )
        .try_collect::<Vec<_>>()
        .await?;

    let result = &docker
        .inspect_container(
            "integration_test_update_container",
            None::<InspectContainerOptions>,
        )
        .await?;

    assert_eq!("exited", result.state.status);

    &docker
        .remove_container(
            "integration_test_update_container",
            None::<RemoveContainerOptions>,
        )
        .await?;

    Ok(())
}

async fn rename_container_test(docker: Docker) -> Result<(), Error> {
    create_container_hello_world(&docker, "integration_test_rename_container").await?;
    &docker
        .rename_container(
            "integration_test_rename_container",
            RenameContainerOptions {
                name: "integration_test_rename_container_renamed".to_string(),
            },
        )
        .await?;

    &docker
        .remove_container(
            "integration_test_rename_container_renamed",
            None::<RemoveContainerOptions>,
        )
        .await?;

    Ok(())
}

async fn pause_container_test(docker: Docker) -> Result<(), Error> {
    create_daemon(&docker, "integration_test_pause_container").await?;

    &docker
        .pause_container("integration_test_pause_container")
        .await?;

    let result = &docker
        .inspect_container(
            "integration_test_pause_container",
            None::<InspectContainerOptions>,
        )
        .await?;

    assert_eq!("paused".to_string(), result.state.status);

    &docker
        .unpause_container("integration_test_pause_container")
        .await?;

    let result = &docker
        .inspect_container(
            "integration_test_pause_container",
            None::<InspectContainerOptions>,
        )
        .await?;

    assert_eq!("running".to_string(), result.state.status);

    kill_container(&docker, "integration_test_pause_container").await?;

    Ok(())
}

async fn prune_containers_test(docker: Docker) -> Result<(), Error> {
    &docker
        .prune_containers(None::<PruneContainersOptions<String>>)
        .await?;

    let result = &docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await?;

    assert_eq!(
        0,
        result
            .into_iter()
            .filter(
                |r| vec!["bollard", "registry:2", "stefanscherer/registry-windows"]
                    .into_iter()
                    .all(|v| v.to_string() != r.image)
            )
            .collect::<Vec<_>>()
            .len()
    );

    Ok(())
}

async fn archive_container_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}microsoft/nanoserver", registry_http_addr())
    } else {
        format!("{}alpine", registry_http_addr())
    };

    let readme = r#"Hello from Bollard!"#.as_bytes();

    let mut header = tar::Header::new_gnu();
    header.set_path("readme.txt").unwrap();
    header.set_size(readme.len() as u64);
    header.set_mode(0o744);
    header.set_cksum();
    let mut tar = tar::Builder::new(Vec::new());
    tar.append(&header, readme).unwrap();

    let uncompressed = tar.into_inner().unwrap();
    let mut c = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    c.write_all(&uncompressed).unwrap();
    let payload = c.finish().unwrap();

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

    &docker
        .create_container(
            Some(CreateContainerOptions {
                name: "integration_test_archive_container",
            }),
            Config {
                image: Some(&image[..]),
                ..Default::default()
            },
        )
        .await?;

    &docker
        .upload_to_container(
            "integration_test_archive_container",
            Some(UploadToContainerOptions {
                path: if cfg!(windows) {
                    "C:\\Windows\\Logs"
                } else {
                    "/tmp"
                },
                ..Default::default()
            }),
            payload.into(),
        )
        .await?;

    let res = docker
        .download_from_container(
            "integration_test_archive_container",
            Some(DownloadFromContainerOptions {
                path: if cfg!(windows) {
                    "C:\\Windows\\Logs\\"
                } else {
                    "/tmp"
                },
            }),
        );

    let bytes = concat_byte_stream(res).await?;

    let mut a: tar::Archive<&[u8]> = tar::Archive::new(&bytes[..]);

    use std::io::Read;
    let files: Vec<String> = a
        .entries()
        .unwrap()
        .map(|file| file.unwrap())
        .filter(|file| {
            let path = file.header().path().unwrap();
            println!("{:?}", path);
            if path
                == std::path::Path::new(if cfg!(windows) {
                    "Logs/readme.txt"
                } else {
                    "tmp/readme.txt"
                })
            {
                return true;
            }
            false
        })
        .map(|mut r| {
            let mut s = String::new();
            r.read_to_string(&mut s).unwrap();
            s
        })
        .collect();

    assert_eq!(1, files.len());

    assert_eq!("Hello from Bollard!", files.first().unwrap());

    &docker
        .remove_container(
            "integration_test_archive_container",
            None::<RemoveContainerOptions>,
        )
        .await?;

    Ok(())
}

async fn inspect_container_test(docker: Docker) -> Result<(), Error> {
    create_daemon(&docker, "integration_test_inspect_container").await?;
    let result = &docker
        .inspect_container(
            "integration_test_inspect_container",
            None::<InspectContainerOptions>,
        )
        .await?;

    assert_eq!(None, result.host_config.capabilities);

    kill_container(&docker, "integration_test_inspect_container").await?;

    Ok(())
}

async fn mount_volume_container_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}microsoft/nanoserver", registry_http_addr())
    } else {
        format!("{}alpine", registry_http_addr())
    };

    let host_config = HostConfig {
        mounts: Some(vec![MountPoint {
            target: if cfg!(windows) {
                "C:\\Windows\\Temp"
            } else {
                "/tmp"
            },
            source: if cfg!(windows) {
                "C:\\Windows\\Temp"
            } else {
                "/tmp"
            },
            type_: "bind",
            consistency: Some("default"),
            ..Default::default()
        }]),
        ..Default::default()
    };

    &docker.create_image(
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
    );

    &docker
        .create_container(
            Some(CreateContainerOptions {
                name: "integration_test_mount_volume_container",
            }),
            Config {
                image: Some(&image[..]),
                host_config: Some(host_config),
                ..Default::default()
            },
        )
        .await?;

    let result = &docker
        .inspect_container(
            "integration_test_mount_volume_container",
            None::<InspectContainerOptions>,
        )
        .await?;

    assert_eq!(
        if cfg!(windows) {
            "C:\\Windows\\Temp".to_string()
        } else {
            "/tmp".to_string()
        },
        result
            .host_config
            .mounts
            .as_ref()
            .unwrap()
            .first()
            .unwrap()
            .target
    );

    &docker
        .remove_container(
            "integration_test_mount_volume_container",
            None::<RemoveContainerOptions>,
        )
        .await?;

    Ok(())
}

#[test]
fn integration_test_list_containers() {
    connect_to_docker_and_run!(list_containers_test);
}

#[test]
fn integration_test_image_push() {
    connect_to_docker_and_run!(image_push_test);
}

#[test]
fn integration_test_restart_container() {
    connect_to_docker_and_run!(container_restart_test);
}

#[test]
fn integration_test_top_processes() {
    connect_to_docker_and_run!(top_processes_test);
}

#[test]
// #[cfg(not(windows))]
// // This works on windows, but is flaky for some reason.
fn integration_test_logs() {
    connect_to_docker_and_run!(logs_test);
}

#[test]
fn integration_test_container_changes() {
    connect_to_docker_and_run!(container_changes_test);
}

#[test]
fn integration_test_stats() {
    connect_to_docker_and_run!(stats_test);
}

#[test]
fn integration_test_kill_container() {
    connect_to_docker_and_run!(kill_container_test);
}

// note: resource updating isn't supported on Windows
#[test]
#[cfg(not(windows))]
fn integration_test_update_container() {
    connect_to_docker_and_run!(update_container_test);
}

#[test]
fn integration_test_rename_container() {
    connect_to_docker_and_run!(rename_container_test);
}

// note: cannot pause Windows Server Containers
#[test]
#[cfg(not(windows))]
fn integration_test_pause_container() {
    connect_to_docker_and_run!(pause_container_test);
}

#[test]
fn integration_test_prune_containers() {
    connect_to_docker_and_run!(prune_containers_test);
}

#[test]
fn integration_test_archive_containers() {
    connect_to_docker_and_run!(archive_container_test);
}

#[test]
fn integration_test_inspect_containers() {
    connect_to_docker_and_run!(inspect_container_test);
}

#[test]
fn integration_test_mount_volume_containers() {
    connect_to_docker_and_run!(mount_volume_container_test);
}
