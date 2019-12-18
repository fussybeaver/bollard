#![type_length_limit = "2097152"]

use futures_util::stream::TryStreamExt;
use tokio::runtime::Runtime;

use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    WaitContainerOptions,
};
use bollard::errors::Error;
use bollard::image::*;
use bollard::Docker;

use std::collections::HashMap;
use std::default::Default;
use std::io::Write;

#[macro_use]
pub mod common;
use crate::common::*;

async fn create_image_test(docker: Docker) -> Result<(), Error> {
    create_image_hello_world(&docker).await?;

    Ok(())
}

async fn search_images_test(docker: Docker) -> Result<(), Error> {
    let result = &docker
        .search_images(SearchImagesOptions {
            term: "hello-world",
            ..Default::default()
        })
        .await?;

    assert!(result
        .into_iter()
        .any(|api_image| &api_image.name == "hello-world"));

    Ok::<_, Error>(())
}

async fn inspect_image_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    create_image_hello_world(&docker).await?;

    let result = &docker.inspect_image(&image).await?;

    assert!(result.repo_tags.iter().any(|repo_tag| repo_tag == &image));

    Ok(())
}

async fn list_images_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    create_image_hello_world(&docker).await?;

    let result = &docker
        .list_images(Some(ListImagesOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await?;

    assert!(result.into_iter().any(|api_image| {
        api_image
            .repo_tags
            .as_ref()
            .unwrap_or(&vec![String::new()])
            .into_iter()
            .any(|repo_tag| repo_tag == &image)
    }));

    Ok(())
}

async fn image_history_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    create_image_hello_world(&docker).await?;

    let result = &docker.image_history(&image).await?;

    assert!(result.iter().take(1).any(|history| history
        .tags
        .as_ref()
        .unwrap_or(&vec![String::new()])
        .iter()
        .any(|tag| tag == &image)));

    Ok(())
}

async fn prune_images_test(docker: Docker) -> Result<(), Error> {
    let mut filters = HashMap::new();
    filters.insert("label", vec!["maintainer=some_maintainer"]);
    &docker
        .prune_images(Some(PruneImagesOptions { filters: filters }))
        .await?;

    Ok(())
}

async fn remove_image_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    create_image_hello_world(&docker).await?;

    let result = &docker
        .remove_image(
            &image,
            Some(RemoveImageOptions {
                noprune: true,
                ..Default::default()
            }),
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .await?;

    assert!(result.iter().any(|s| match s {
        RemoveImageResults::RemoveImageUntagged { untagged } => untagged == &image,
        _ => false,
    }));

    Ok(())
}

async fn commit_container_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}microsoft/nanoserver", registry_http_addr())
    } else {
        format!("{}alpine", registry_http_addr())
    };

    let cmd = if cfg!(windows) {
        Some(vec!["cmd.exe", "/C", "copy", "nul", "bollard.txt"])
    } else {
        Some(vec!["touch", "/bollard.txt"])
    };

    create_image_hello_world(&docker).await?;

    &docker
        .create_container(
            Some(CreateContainerOptions {
                name: "integration_test_commit_container",
            }),
            Config {
                cmd: cmd,
                image: Some(&image[..]),
                ..Default::default()
            },
        )
        .await?;

    &docker
        .start_container(
            "integration_test_commit_container",
            None::<StartContainerOptions<String>>,
        )
        .await?;

    &docker
        .wait_container(
            "integration_test_commit_container",
            None::<WaitContainerOptions<String>>,
        )
        .try_collect::<Vec<_>>()
        .await?;

    &docker
        .commit_container(
            CommitContainerOptions {
                container: "integration_test_commit_container",
                repo: "integration_test_commit_container_next",
                pause: true,
                ..Default::default()
            },
            Config::<String> {
                ..Default::default()
            },
        )
        .await?;

    &docker
        .create_container(
            Some(CreateContainerOptions {
                name: "integration_test_commit_container_next",
            }),
            Config {
                image: Some("integration_test_commit_container_next"),
                cmd: if cfg!(windows) {
                    Some(vec!["cmd.exe", "/C", "dir", "bollard.txt"])
                } else {
                    Some(vec!["ls", "/bollard.txt"])
                },
                ..Default::default()
            },
        )
        .await?;

    &docker
        .start_container(
            "integration_test_commit_container_next",
            None::<StartContainerOptions<String>>,
        )
        .await?;

    let vec = &docker
        .wait_container(
            "integration_test_commit_container_next",
            None::<WaitContainerOptions<String>>,
        )
        .try_collect::<Vec<_>>()
        .await?;

    let first = vec.get(0).unwrap();
    if let Some(error) = &first.error {
        println!("{}", error.message);
    }
    assert_eq!(first.status_code, 0);

    &docker
        .remove_container(
            "integration_test_commit_container_next",
            None::<RemoveContainerOptions>,
        )
        .await?;

    &docker
        .remove_image(
            "integration_test_commit_container_next",
            None::<RemoveImageOptions>,
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .await?;

    &docker
        .remove_container(
            "integration_test_commit_container",
            None::<RemoveContainerOptions>,
        )
        .await?;

    Ok(())
}

async fn build_image_test(docker: Docker) -> Result<(), Error> {
    let dockerfile = if cfg!(windows) {
        format!(
            "FROM {}microsoft/nanoserver
RUN cmd.exe /C copy nul bollard.txt
",
            registry_http_addr()
        )
    } else {
        format!(
            "FROM {}alpine
RUN touch bollard.txt
",
            registry_http_addr()
        )
    };
    let mut header = tar::Header::new_gnu();
    header.set_path("Dockerfile").unwrap();
    header.set_size(dockerfile.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    let mut tar = tar::Builder::new(Vec::new());
    tar.append(&header, dockerfile.as_bytes()).unwrap();

    let uncompressed = tar.into_inner().unwrap();
    let mut c = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    c.write_all(&uncompressed).unwrap();
    let compressed = c.finish().unwrap();

    let mut creds = HashMap::new();
    creds.insert(
        "localhost:5000".to_string(),
        integration_test_registry_credentials(),
    );

    &docker
        .build_image(
            BuildImageOptions {
                dockerfile: "Dockerfile".to_string(),
                t: "integration_test_build_image".to_string(),
                pull: true,
                rm: true,
                ..Default::default()
            },
            if cfg!(windows) { None } else { Some(creds) },
            Some(compressed.into()),
        )
        .try_collect::<Vec<_>>()
        .await?;

    &docker
        .create_container(
            Some(CreateContainerOptions {
                name: "integration_test_build_image",
            }),
            Config {
                image: Some("integration_test_build_image"),
                cmd: if cfg!(windows) {
                    Some(vec!["cmd.exe", "/C", "dir", "bollard.txt"])
                } else {
                    Some(vec!["ls", "/bollard.txt"])
                },
                ..Default::default()
            },
        )
        .await?;

    &docker
        .start_container(
            "integration_test_build_image",
            None::<StartContainerOptions<String>>,
        )
        .await?;

    let vec = &docker
        .wait_container(
            "integration_test_build_image",
            None::<WaitContainerOptions<String>>,
        )
        .try_collect::<Vec<_>>()
        .await?;

    let first = vec.get(0).unwrap();
    if let Some(error) = &first.error {
        println!("{}", error.message);
    }
    assert_eq!(first.status_code, 0);
    &docker
        .remove_container(
            "integration_test_build_image",
            None::<RemoveContainerOptions>,
        )
        .await?;

    &docker
        .remove_image(
            "integration_test_build_image",
            None::<RemoveImageOptions>,
            if cfg!(windows) {
                None
            } else {
                Some(integration_test_registry_credentials())
            },
        )
        .await?;

    Ok(())
}

#[test]
fn integration_test_search_images() {
    connect_to_docker_and_run!(search_images_test);
}

#[test]
fn integration_test_create_image() {
    connect_to_docker_and_run!(create_image_test);
}

#[test]
fn integration_test_inspect_image() {
    connect_to_docker_and_run!(inspect_image_test);
}

#[test]
fn integration_test_images_list() {
    connect_to_docker_and_run!(list_images_test);
}

#[test]
fn integration_test_image_history() {
    connect_to_docker_and_run!(image_history_test);
}

#[test]
fn integration_test_prune_images() {
    connect_to_docker_and_run!(prune_images_test);
}

#[test]
fn integration_test_remove_image() {
    connect_to_docker_and_run!(remove_image_test);
}

#[test]
fn integration_test_commit_container() {
    connect_to_docker_and_run!(commit_container_test);
}

#[test]
fn integration_test_build_image() {
    connect_to_docker_and_run!(build_image_test);
}
