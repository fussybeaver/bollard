#![type_length_limit = "2097152"]
extern crate bollard;
extern crate failure;
extern crate flate2;
extern crate futures;
extern crate hyper;
#[cfg(unix)]
extern crate hyperlocal;
extern crate tar;
extern crate tokio;

use bollard::container::*;
use bollard::image::*;
use bollard::Docker;

use hyper::rt::{Future, Stream};
use tokio::runtime::Runtime;

use std::io::Write;

#[macro_use]
pub mod common;
use crate::common::*;

fn list_containers_test(docker: Docker) {
    let image = move || {
        if cfg!(windows) {
            format!("{}hello-world:nanoserver", registry_http_addr())
        } else {
            format!("{}hello-world:linux", registry_http_addr())
        }
    };

    let rt = Runtime::new().unwrap();
    let future =
        chain_create_container_hello_world(docker.chain(), "integration_test_list_containers")
            .and_then(move |docker| {
                docker.list_containers(Some(ListContainersOptions::<String> {
                    all: true,
                    ..Default::default()
                }))
            })
            .map(move |(docker, result)| {
                assert_ne!(0, result.len());
                assert!(result
                    .into_iter()
                    .any(|container| container.image == image()));
                docker
            })
            .and_then(move |docker| {
                docker.remove_container(
                    "integration_test_list_containers",
                    None::<RemoveContainerOptions>,
                )
            });

    run_runtime(rt, future);
}

fn image_push_test(docker: Docker) {
    let image = move || {
        if cfg!(windows) {
            format!("{}hello-world:nanoserver", registry_http_addr())
        } else {
            format!("{}hello-world:linux", registry_http_addr())
        }
    };

    let rt = Runtime::new().unwrap();

    let future = docker.chain();

    let future = future
        .create_image(
            Some(CreateImageOptions {
                from_image: image(),
                ..Default::default()
            }),
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .and_then(move |(docker, _)| {
            docker.tag_image(
                &image(),
                Some(TagImageOptions {
                    repo: format!("{}my-hello-world", registry_http_addr()),
                    ..Default::default()
                }),
            )
        })
        .and_then(move |(docker, _)| {
            docker.push_image(
                format!("{}my-hello-world", registry_http_addr()).as_ref(),
                None::<PushImageOptions<String>>,
                if cfg!(windows) {
                    None
                } else {
                    Some(integration_test_registry_credentials())
                },
            )
        });

    run_runtime(rt, future);
}

fn container_restart_test(docker: Docker) {
    let rt = Runtime::new().unwrap();
    let future = chain_create_daemon(docker.chain(), "integration_test_restart_container");

    let future = future
        .and_then(|docker| {
            docker.inspect_container(
                "integration_test_restart_container",
                None::<InspectContainerOptions>,
            )
        })
        .map(|(docker, result)| (docker, result.state.started_at))
        .and_then(|(docker, started_at)| {
            docker
                .restart_container(
                    "integration_test_restart_container",
                    None::<RestartContainerOptions>,
                )
                .map(move |(docker, _)| (docker, started_at))
        })
        .and_then(|(docker, started_at)| {
            docker
                .inspect_container(
                    "integration_test_restart_container",
                    None::<InspectContainerOptions>,
                )
                .map(move |(docker, result)| {
                    assert_ne!(started_at, result.state.started_at);
                    (docker, result)
                })
        })
        .and_then(move |(docker, _)| {
            chain_kill_container(docker, "integration_test_restart_container")
        });

    run_runtime(rt, future);
}

fn top_processes_test(docker: Docker) {
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

    let rt = Runtime::new().unwrap();
    let future = chain_create_daemon(docker.chain(), "integration_test_top_processes")
        .and_then(|docker| docker.top_processes("integration_test_top_processes", top_options))
        .map(move |(docker, result)| {
            assert_eq!(result.titles[0], expected);
            docker
        })
        .and_then(|docker| chain_kill_container(docker, "integration_test_top_processes"));

    run_runtime(rt, future);
}

fn logs_test(docker: Docker) {
    let rt = Runtime::new().unwrap();
    let docker2 = docker.clone();
    let future = chain_create_container_hello_world(docker.chain(), "integration_test_logs")
        .and_then(|docker| {
            docker.logs(
                "integration_test_logs",
                Some(LogsOptions {
                    follow: true,
                    stdout: true,
                    stderr: false,
                    tail: "all".to_string(),
                    ..Default::default()
                }),
            )
        })
        .map(|(_, stream)| {
            stream
                .skip(1)
                .into_future()
                .map(|(value, _)| {
                    assert_eq!(
                        format!("{}", value.unwrap()),
                        "Hello from Docker!".to_string()
                    );
                })
                .or_else(|e| {
                    println!("{}", e.0);
                    Err(e.0)
                })
        })
        .flatten()
        .and_then(move |_| {
            docker2.remove_container("integration_test_logs", None::<RemoveContainerOptions>)
        });

    run_runtime(rt, future);
}

fn container_changes_test(docker: Docker) {
    let rt = Runtime::new().unwrap();
    let future =
        chain_create_container_hello_world(docker.chain(), "integration_test_container_changes")
            .and_then(|docker| docker.container_changes("integration_test_container_changes"))
            .map(|(docker, result)| {
                if cfg!(windows) {
                    assert_ne!(result.unwrap().len(), 0)
                } else {
                    assert!(result.is_none())
                };

                docker
            })
            .and_then(|docker| {
                docker.remove_container(
                    "integration_test_container_changes",
                    None::<RemoveContainerOptions>,
                )
            });

    run_runtime(rt, future);
}

fn stats_test(docker: Docker) {
    let rt = Runtime::new().unwrap();
    let future = chain_create_daemon(docker.chain(), "integration_test_stats")
        .and_then(|docker| {
            docker.stats(
                "integration_test_stats",
                Some(StatsOptions { stream: false }),
            )
        })
        .map(|(docker, stream)| {
            stream
                .into_future()
                .map(|(value, _)| {
                    assert_eq!(value.unwrap().name, "/integration_test_stats".to_string())
                })
                .or_else(|e| {
                    println!("{}", e.0);
                    Err(e.0)
                })
                .wait()
                .unwrap();
            docker
        })
        .and_then(|docker| chain_kill_container(docker, "integration_test_stats"));

    run_runtime(rt, future);
}

fn kill_container_test(docker: Docker) {
    let kill_options = Some(KillContainerOptions { signal: "SIGKILL" });

    let rt = Runtime::new().unwrap();
    let future = chain_create_daemon(docker.chain(), "integration_test_kill_container")
        .and_then(|docker| docker.kill_container("integration_test_kill_container", kill_options))
        .and_then(|(docker, _)| {
            docker.remove_container(
                "integration_test_kill_container",
                None::<RemoveContainerOptions>,
            )
        });

    run_runtime(rt, future);
}

fn update_container_test(docker: Docker) {
    let update_options = UpdateContainerOptions {
        memory: Some(314572800),
        memory_swap: Some(314572800),
        ..Default::default()
    };

    let rt = Runtime::new().unwrap();
    let future = chain_create_daemon(docker.chain(), "integration_test_update_container")
        .and_then(|docker| {
            docker.update_container("integration_test_update_container", update_options)
        })
        .and_then(|(docker, _)| {
            docker.inspect_container(
                "integration_test_update_container",
                None::<InspectContainerOptions>,
            )
        })
        .map(|(docker, result)| {
            assert_eq!(314572800, result.host_config.memory.unwrap());
            docker
        })
        .and_then(|docker| {
            docker.kill_container(
                "integration_test_update_container",
                None::<KillContainerOptions<String>>,
            )
        })
        .and_then(|(docker, _)| {
            docker.wait_container(
                "integration_test_update_container",
                None::<WaitContainerOptions<String>>,
            )
        })
        .and_then(|(docker, _)| {
            docker.inspect_container(
                "integration_test_update_container",
                None::<InspectContainerOptions>,
            )
        })
        .map(|(docker, result)| {
            assert_eq!("exited", result.state.status);
            docker
        })
        .and_then(|docker| {
            docker.remove_container(
                "integration_test_update_container",
                None::<RemoveContainerOptions>,
            )
        });

    run_runtime(rt, future);
}

fn rename_container_test(docker: Docker) {
    let rt = Runtime::new().unwrap();
    let future =
        chain_create_container_hello_world(docker.chain(), "integration_test_rename_container")
            .and_then(|docker| {
                docker.rename_container(
                    "integration_test_rename_container",
                    RenameContainerOptions {
                        name: "integration_test_rename_container_renamed".to_string(),
                    },
                )
            })
            .and_then(|(docker, _)| {
                docker.remove_container(
                    "integration_test_rename_container_renamed",
                    None::<RemoveContainerOptions>,
                )
            });

    run_runtime(rt, future);
}

fn pause_container_test(docker: Docker) {
    let rt = Runtime::new().unwrap();
    let future = chain_create_daemon(docker.chain(), "integration_test_pause_container")
        .and_then(|docker| docker.pause_container("integration_test_pause_container"))
        .and_then(|(docker, _)| {
            docker.inspect_container(
                "integration_test_pause_container",
                None::<InspectContainerOptions>,
            )
        })
        .map(|(docker, result)| {
            assert_eq!("paused".to_string(), result.state.status);
            docker
        })
        .and_then(|docker| docker.unpause_container("integration_test_pause_container"))
        .and_then(|(docker, _)| {
            docker.inspect_container(
                "integration_test_pause_container",
                None::<InspectContainerOptions>,
            )
        })
        .map(|(docker, result)| {
            assert_eq!("running".to_string(), result.state.status);
            docker
        })
        .and_then(|docker| chain_kill_container(docker, "integration_test_pause_container"));

    run_runtime(rt, future);
}

fn prune_containers_test(docker: Docker) {
    let rt = Runtime::new().unwrap();
    let future = docker
        .chain()
        .prune_containers(None::<PruneContainersOptions<String>>)
        .and_then(|(docker, _)| {
            docker.list_containers(Some(ListContainersOptions::<String> {
                all: true,
                ..Default::default()
            }))
        })
        .map(|(docker, result)| {
            println!("{:?}", result.iter().map(|c| c.clone().names));
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
            docker
        });

    run_runtime(rt, future);
}

fn archive_container_test(docker: Docker) {
    let rt = Runtime::new().unwrap();

    let image = move || {
        if cfg!(windows) {
            format!("{}microsoft/nanoserver", registry_http_addr())
        } else {
            format!("{}alpine", registry_http_addr())
        }
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

    let cleanup = docker.clone();

    let future = docker
        .chain()
        .create_image(
            Some(CreateImageOptions {
                from_image: image(),
                ..Default::default()
            }),
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .and_then(move |(docker, _)| {
            docker.create_container(
                Some(CreateContainerOptions {
                    name: "integration_test_archive_container",
                }),
                Config {
                    image: Some(image()),
                    ..Default::default()
                },
            )
        })
        .and_then(|(docker, _)| {
            docker.upload_to_container(
                "integration_test_archive_container",
                Some(UploadToContainerOptions {
                    path: "/tmp",
                    ..Default::default()
                }),
                payload.into(),
            )
        })
        .and_then(|(docker, _)| {
            docker.download_from_container(
                "integration_test_archive_container",
                Some(DownloadFromContainerOptions { path: "/tmp" }),
            )
        })
        .and_then(|(_, stream)| stream.concat2())
        .map(|chunk| {
            let bytes = &chunk.into_bytes()[..];
            let mut a: tar::Archive<&[u8]> = tar::Archive::new(bytes);

            use std::io::Read;
            let files: Vec<String> = a
                .entries()
                .unwrap()
                .map(|file| file.unwrap())
                .filter(|file| {
                    let path = file.header().path().unwrap();
                    if path == std::path::Path::new("tmp/readme.txt") {
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
        })
        .then(move |_| {
            cleanup.remove_container(
                "integration_test_archive_container",
                None::<RemoveContainerOptions>,
            )
        });

    run_runtime(rt, future);
}

fn inspect_container_test(docker: Docker) {
    let rt = Runtime::new().unwrap();
    let future = chain_create_daemon(docker.chain(), "integration_test_inspect_container")
        .and_then(|docker| {
            docker.inspect_container(
                "integration_test_inspect_container",
                None::<InspectContainerOptions>,
            )
        })
        .map(|(docker, result)| {
            assert_eq!(None, result.host_config.capabilities);
            docker
        })
        .and_then(|docker| chain_kill_container(docker, "integration_test_inspect_container"));

    run_runtime(rt, future);
}

fn mount_volume_container_test(docker: Docker) {
    let rt = Runtime::new().unwrap();

    let image = move || {
        if cfg!(windows) {
            format!("{}microsoft/nanoserver", registry_http_addr())
        } else {
            format!("{}alpine", registry_http_addr())
        }
    };

    let host_config = HostConfig {
        mounts: Some(vec![MountPoint {
            target: if cfg!(windows) {
                "C:\\Windows\\Temp".to_string()
            } else {
                "/tmp".to_string()
            },
            source: if cfg!(windows) {
                "C:\\Windows\\Temp".to_string()
            } else {
                "/tmp".to_string()
            },
            type_: "bind".to_string(),
            consistency: "default".to_string(),
            ..Default::default()
        }]),
        ..Default::default()
    };

    let future = docker
        .chain()
        .create_image(
            Some(CreateImageOptions {
                from_image: image(),
                ..Default::default()
            }),
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .and_then(move |(docker, _)| {
            docker.create_container(
                Some(CreateContainerOptions {
                    name: "integration_test_mount_volume_container",
                }),
                Config {
                    image: Some(image()),
                    host_config: Some(host_config),
                    ..Default::default()
                },
            )
        })
        .and_then(|(docker, _)| {
            docker.inspect_container(
                "integration_test_mount_volume_container",
                None::<InspectContainerOptions>,
            )
        })
        .map(|(docker, result)| {
            assert_eq!(
                if cfg!(windows) {
                    "C:\\Windows\\Temp".to_string()
                } else {
                    "/tmp".to_string()
                },
                result.host_config.mounts.unwrap().first().unwrap().target
            );
            docker
        })
        .and_then(move |docker| {
            docker.remove_container(
                "integration_test_mount_volume_container",
                None::<RemoveContainerOptions>,
            )
        });

    run_runtime(rt, future);
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
#[cfg(not(windows))]
// This works on windows, but is flaky for some reason.
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
