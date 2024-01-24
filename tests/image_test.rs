#![type_length_limit = "2097152"]

use bytes::BufMut;
use futures_util::future::ready;
use futures_util::stream::{StreamExt, TryStreamExt};
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
use std::fs::{remove_file, File};
use std::io::Write;

#[macro_use]
pub mod common;
use crate::common::*;

async fn create_image_test(docker: Docker) -> Result<(), Error> {
    create_image_hello_world(&docker).await?;

    Ok(())
}

async fn create_image_wasm_test(docker: Docker) -> Result<(), Error> {
    let image = "empty-wasm:latest";

    let options = CreateImageOptions {
        from_src: "-", // from_src must be "-" when sending the archive in the request body
        repo: image,
        ..Default::default()
    };

    let req_body = bytes::Bytes::from({
        let mut buffer = Vec::new();

        {
            let mut builder = tar::Builder::new(&mut buffer);
            let mut header = tar::Header::new_gnu();
            header.set_path("entrypoint.wasm")?;
            header.set_size(0);
            header.set_cksum();

            builder.append_data(&mut header, "entrypoint.wasm", [].as_slice())?;
            builder.finish()?;
        }

        buffer
    });

    docker
        .create_image(Some(options), Some(req_body), None)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, bollard::errors::Error>>()
        .unwrap();

    let result = &docker.inspect_image(image).await?;

    assert!(result.repo_tags.as_ref().unwrap() == [image.to_owned()].as_slice());

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
        .iter()
        .any(|api_image| &api_image.name.as_ref().unwrap()[..] == "hello-world"));

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

    assert!(result
        .repo_tags
        .as_ref()
        .unwrap()
        .iter()
        .any(|repo_tag| repo_tag == &image));

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

    assert!(result.iter().any(|api_image| {
        api_image
            .repo_tags
            .iter()
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

    assert!(result
        .iter()
        .take(1)
        .any(|history| history.tags.iter().any(|tag| tag == &image)));

    Ok(())
}

async fn prune_images_test(docker: Docker) -> Result<(), Error> {
    let mut filters = HashMap::new();
    filters.insert("label", vec!["maintainer=some_maintainer"]);
    let _ = &docker
        .prune_images(Some(PruneImagesOptions { filters }))
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

    assert!(result.iter().any(|s| s
        .untagged
        .as_ref()
        .map(|untagged| untagged == &image)
        .unwrap_or(false)));

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

    let _ = &docker
        .create_container(
            Some(CreateContainerOptions {
                name: "integration_test_commit_container",
                platform: None,
            }),
            Config {
                cmd,
                image: Some(&image[..]),
                ..Default::default()
            },
        )
        .await?;

    let _ = &docker
        .start_container(
            "integration_test_commit_container",
            None::<StartContainerOptions<String>>,
        )
        .await?;

    let _ = &docker
        .wait_container(
            "integration_test_commit_container",
            None::<WaitContainerOptions<String>>,
        )
        .try_collect::<Vec<_>>()
        .await?;

    let _ = &docker
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

    let _ = &docker
        .create_container(
            Some(CreateContainerOptions {
                name: "integration_test_commit_container_next",
                platform: None,
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

    let _ = &docker
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

    let first = vec.first().unwrap();
    if let Some(error) = &first.error {
        println!("{}", error.message.as_ref().unwrap());
    }
    assert_eq!(first.status_code, 0);

    let _ = &docker
        .remove_container(
            "integration_test_commit_container_next",
            None::<RemoveContainerOptions>,
        )
        .await?;

    let _ = &docker
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

    let _ = &docker
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

    let _ = &docker
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

    let _ = &docker
        .create_container(
            Some(CreateContainerOptions {
                name: "integration_test_build_image",
                platform: None,
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

    let _ = &docker
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

    let first = vec.first().unwrap();
    if let Some(error) = &first.error {
        println!("{}", error.message.as_ref().unwrap());
    }
    assert_eq!(first.status_code, 0);
    let _ = &docker
        .remove_container("integration_test_build_image", None)
        .await?;

    let _ = &docker
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

#[cfg(feature = "buildkit")]
async fn build_buildkit_image_test(docker: Docker) -> Result<(), Error> {
    let dockerfile = String::from(
        "FROM localhost:5000/alpine as builder1
RUN touch bollard.txt
FROM localhost:5000/alpine as builder2
RUN --mount=type=bind,from=builder1,target=mnt cp mnt/bollard.txt buildkit-bollard.txt
ENTRYPOINT ls buildkit-bollard.txt
",
    );
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

    let credentials = bollard::auth::DockerCredentials {
        username: Some("bollard".to_string()),
        password: std::env::var("REGISTRY_PASSWORD").ok(),
        ..Default::default()
    };
    let mut creds_hsh = std::collections::HashMap::new();
    creds_hsh.insert("localhost:5000".to_string(), credentials);

    let id = "build_buildkit_image_test";
    let build = &docker
        .build_image(
            BuildImageOptions {
                dockerfile: "Dockerfile".to_string(),
                t: "integration_test_build_buildkit_image".to_string(),
                pull: true,
                version: BuilderVersion::BuilderBuildKit,
                rm: true,
                #[cfg(feature = "buildkit")]
                session: Some(String::from(id)),
                ..Default::default()
            },
            Some(creds_hsh),
            Some(compressed.into()),
        )
        .try_collect::<Vec<bollard::models::BuildInfo>>()
        .await?;

    assert!(build
        .iter()
        .flat_map(|build_info| {
            if let Some(aux) = &build_info.aux {
                match aux {
                    bollard::models::BuildInfoAux::BuildKit(res) => Vec::clone(&res.statuses),
                    _ => vec![],
                }
            } else {
                vec![]
            }
        })
        .any(|status| status.id
            == "naming to docker.io/library/integration_test_build_buildkit_image:latest"));

    let _ = &docker
        .create_container(
            Some(CreateContainerOptions {
                name: "integration_test_build_buildkit_image",
                platform: None,
            }),
            Config {
                image: Some("integration_test_build_buildkit_image"),
                cmd: Some(vec!["ls", "/buildkit-bollard.txt"]),
                ..Default::default()
            },
        )
        .await?;

    let _ = &docker
        .start_container(
            "integration_test_build_buildkit_image",
            None::<StartContainerOptions<String>>,
        )
        .await?;

    let vec = &docker
        .wait_container(
            "integration_test_build_buildkit_image",
            None::<WaitContainerOptions<String>>,
        )
        .try_collect::<Vec<_>>()
        .await?;

    let first = vec.first().unwrap();
    if let Some(error) = &first.error {
        println!("{}", error.message.as_ref().unwrap());
    }
    assert_eq!(first.status_code, 0);
    let _ = &docker
        .remove_container("integration_test_build_buildkit_image", None)
        .await?;

    let _ = &docker
        .remove_image(
            "integration_test_build_buildkit_image",
            None::<RemoveImageOptions>,
            None,
        )
        .await?;

    Ok(())
}

#[cfg(feature = "buildkit")]
async fn buildkit_image_missing_session_test(docker: Docker) -> Result<(), Error> {
    let dockerfile = String::from(
        "FROM alpine as builder1
RUN touch bollard.txt
FROM alpine as builder2
RUN --mount=type=bind,from=builder1,target=mnt cp mnt/bollard.txt buildkit-bollard.txt
ENTRYPOINT ls buildkit-bollard.txt
",
    );
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

    let build = &docker
        .build_image(
            BuildImageOptions {
                dockerfile: "Dockerfile".to_string(),
                t: "integration_test_build_buildkit_image".to_string(),
                pull: true,
                version: BuilderVersion::BuilderBuildKit,
                rm: true,
                #[cfg(feature = "buildkit")]
                session: None,
                ..Default::default()
            },
            None,
            Some(compressed.into()),
        )
        .try_collect::<Vec<bollard::models::BuildInfo>>()
        .await;

    assert!(build.is_err());
    let err = build.as_ref().unwrap_err();
    assert!(matches!(err, Error::MissingSessionBuildkitError {}));

    Ok(())
}

#[cfg(feature = "buildkit")]
async fn build_buildkit_image_inline_driver_test(docker: Docker) -> Result<(), Error> {
    let dockerfile = String::from(
        "FROM localhost:5000/alpine as builder1
RUN touch bollard.txt
FROM localhost:5000/alpine as builder2
RUN --mount=type=bind,from=builder1,target=mnt cp mnt/bollard.txt buildkit-bollard.txt
ENTRYPOINT ls buildkit-bollard.txt
",
    );
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

    let name = "integration_test_build_buildkit_image_inline_driver";

    let credentials = bollard::auth::DockerCredentials {
        username: Some("bollard".to_string()),
        password: std::env::var("REGISTRY_PASSWORD").ok(),
        ..Default::default()
    };
    let mut creds_hsh = std::collections::HashMap::new();
    creds_hsh.insert("localhost:5000".to_string(), credentials);

    let cache_attrs = std::collections::HashMap::new();
    let cache_from = bollard_buildkit_proto::moby::buildkit::v1::CacheOptionsEntry {
        r#type: String::from("inline"),
        attrs: std::collections::HashMap::clone(&cache_attrs),
    };
    let cache_to = bollard_buildkit_proto::moby::buildkit::v1::CacheOptionsEntry {
        r#type: String::from("inline"),
        attrs: cache_attrs,
    };
    let frontend_opts = bollard::grpc::build::ImageBuildFrontendOptions::builder()
        .cachefrom(&cache_from)
        .cacheto(&cache_to)
        .pull(true)
        .build();

    let driver = bollard::grpc::driver::moby::Moby::new(&docker);

    let load_input =
        bollard::grpc::build::ImageBuildLoadInput::Upload(bytes::Bytes::from(compressed));

    let credentials = bollard::auth::DockerCredentials {
        username: Some("bollard".to_string()),
        password: std::env::var("REGISTRY_PASSWORD").ok(),
        ..Default::default()
    };
    let mut creds_hsh = std::collections::HashMap::new();
    creds_hsh.insert("localhost:5000", credentials);

    let res = bollard::grpc::driver::Build::docker_build(
        driver,
        name,
        frontend_opts,
        load_input,
        Some(creds_hsh),
    )
    .await;

    assert!(res.is_ok());

    let _ = &docker
        .create_container(
            Some(CreateContainerOptions {
                name: "integration_test_build_buildkit_image_inline_driver",
                platform: None,
            }),
            Config {
                image: Some("integration_test_build_buildkit_image_inline_driver"),
                cmd: Some(vec!["ls", "/buildkit-bollard.txt"]),
                ..Default::default()
            },
        )
        .await?;

    let _ = &docker
        .start_container(
            "integration_test_build_buildkit_image_inline_driver",
            None::<StartContainerOptions<String>>,
        )
        .await?;

    let vec = &docker
        .wait_container(
            "integration_test_build_buildkit_image_inline_driver",
            None::<WaitContainerOptions<String>>,
        )
        .try_collect::<Vec<_>>()
        .await?;

    let first = vec.first().unwrap();
    if let Some(error) = &first.error {
        println!("{}", error.message.as_ref().unwrap());
    }
    assert_eq!(first.status_code, 0);
    let _ = &docker
        .remove_container("integration_test_build_buildkit_image_inline_driver", None)
        .await?;

    let _ = &docker
        .remove_image(
            "integration_test_build_buildkit_image_inline_driver",
            None::<RemoveImageOptions>,
            None,
        )
        .await?;

    Ok(())
}

async fn export_image_test(docker: Docker) -> Result<(), Error> {
    create_image_hello_world(&docker).await?;

    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };
    let temp_file = if cfg!(windows) {
        "C:\\Users\\appveyor\\Appdata\\Local\\Temp\\bollard_test_image_export.tar"
    } else {
        "/tmp/bollard_test_image_export.tar"
    };

    let res = docker.export_image(&image);

    let mut archive_file = File::create(temp_file).unwrap();
    // Shouldn't load the whole file into memory, stream it to disk instead
    res.for_each(move |data| {
        archive_file.write_all(&data.unwrap()).unwrap();
        archive_file.sync_all().unwrap();
        ready(())
    })
    .await;

    // assert that the file containg the exported archive actually exists
    let test_file = File::open(temp_file).unwrap();
    // and metadata can be read
    test_file.metadata().unwrap();

    // And delete it to clean up
    remove_file(temp_file).unwrap();
    Ok(())
}

async fn export_images_test(docker: Docker) -> Result<(), Error> {
    // pull from registry
    create_image_hello_world(&docker).await?;

    let repo = format!("{}hello-world", registry_http_addr());
    let image = format!("{repo}:linux");

    docker
        .tag_image(
            &image,
            Some(TagImageOptions {
                repo: repo.as_ref(),
                tag: "mycopy",
            }),
        )
        .await?;

    let copy = format!("{repo}:mycopy");
    let images = vec![image.as_ref(), copy.as_ref()];
    let res = docker.export_images(&images);

    let temp_file = "/tmp/bollard_test_images_export.tar";
    let mut archive_file = File::create(temp_file).unwrap();
    // Shouldn't load the whole file into memory, stream it to disk instead
    res.for_each(move |data| {
        archive_file.write_all(&data.unwrap()).unwrap();
        archive_file.sync_all().unwrap();
        ready(())
    })
    .await;

    // assert that the file containing the exported archive actually exists
    let test_file = File::open(temp_file).unwrap();
    // and metadata can be read
    test_file.metadata().unwrap();

    // And delete it to clean up
    remove_file(temp_file).unwrap();
    Ok(())
}

async fn issue_55_test(docker: Docker) -> Result<(), Error> {
    let dockerfile = "FROM ubuntu:18.04
RUN apt-get update && \
    apt-get install -y \
        build-essential \
        cmake \
        curl \
        file \
        git \
        graphviz \
        musl-dev \
        musl-tools \
        libpq-dev \
        libsqlite-dev \
        libssl-dev \
        linux-libc-dev \
        pkgconf \
        sudo \
        xutils-dev \
        gcc-multilib-arm-linux-gnueabihf \
        && \
    apt-get clean && rm -rf /var/lib/apt/lists/*
";
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

    let mut stream = docker.build_image(
        BuildImageOptions {
            dockerfile: "Dockerfile".to_string(),
            t: "issue_55".to_string(),
            pull: true,
            rm: true,
            ..Default::default()
        },
        None,
        Some(compressed.into()),
    );

    while let Some(update) = stream.next().await {
        assert!(update.is_ok());
    }

    Ok(())
}

async fn import_image_test(docker: Docker) -> Result<(), Error> {
    // round-trip test
    create_image_hello_world(&docker).await?;

    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    let mut res = docker.export_image(&image);

    let mut buf = bytes::BytesMut::new();
    while let Some(data) = res.next().await {
        buf.put_slice(&data.unwrap());
    }

    let mut creds = HashMap::new();
    creds.insert(
        "localhost:5000".to_string(),
        integration_test_registry_credentials(),
    );

    docker
        .import_image(
            ImportImageOptions {
                ..Default::default()
            },
            buf.freeze(),
            Some(creds),
        )
        .try_collect::<Vec<_>>()
        .await?;

    Ok(())
}

// ND - Test sometimes hangs on appveyor.
#[cfg(not(windows))]
#[test]
fn integration_test_search_images() {
    connect_to_docker_and_run!(search_images_test);
}

#[test]
fn integration_test_create_image() {
    connect_to_docker_and_run!(create_image_test);
}

#[test]
#[cfg(unix)]
fn integration_test_create_image_wasm() {
    connect_to_docker_and_run!(create_image_wasm_test);
}

#[test]
// ND - Test sometimes hangs on appveyor.
#[cfg(not(windows))]
fn integration_test_inspect_image() {
    connect_to_docker_and_run!(inspect_image_test);
}

#[test]
// ND - Test sometimes hangs on appveyor.
#[cfg(not(windows))]
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
// ND - Test sometimes hangs on appveyor.
#[cfg(not(windows))]
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

#[test]
#[cfg(feature = "buildkit")]
fn integration_test_build_buildkit_image() {
    connect_to_docker_and_run!(build_buildkit_image_test);
}

#[test]
#[cfg(feature = "buildkit")]
fn integration_test_buildkit_image_missing_session_test() {
    connect_to_docker_and_run!(buildkit_image_missing_session_test);
}

#[test]
#[cfg(feature = "buildkit")]
fn integration_test_build_buildkit_inline_driver() {
    connect_to_docker_and_run!(build_buildkit_image_inline_driver_test);
}

#[test]
#[cfg(unix)]
fn integration_test_export_image() {
    connect_to_docker_and_run!(export_image_test);
}

#[test]
#[cfg(unix)]
fn integration_test_export_images() {
    connect_to_docker_and_run!(export_images_test);
}

#[test]
#[cfg(unix)]
// Flaky
#[ignore]
fn integration_test_issue_55() {
    connect_to_docker_and_run!(issue_55_test);
}

#[test]
#[cfg(unix)]
fn integration_test_import_image() {
    connect_to_docker_and_run!(import_image_test);
}
