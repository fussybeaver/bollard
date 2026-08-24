#![cfg(feature = "buildkit")]

use bollard::errors::Error;
use bollard::Docker;

use bollard::grpc::driver::docker_container::{
    DockerContainerBuilder, DockerContainerLifecycle, DockerContainerRemoveOptions,
};
use tokio::runtime::Runtime;

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

#[macro_use]
pub mod common;
use crate::common::*;

#[cfg(feature = "buildkit_providerless")]
fn unique_builder_name(prefix: &str) -> String {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_nanos();
    format!("{prefix}_{suffix}")
}

#[cfg(feature = "buildkit_providerless")]
fn is_not_found(error: &Error) -> bool {
    matches!(
        error,
        Error::DockerResponseServerError {
            status_code: 404,
            ..
        }
    )
}

#[cfg(feature = "buildkit_providerless")]
async fn cleanup_named_builder(docker: &Docker, name: &str, volume_name: &str) {
    use bollard::query_parameters::{RemoveContainerOptionsBuilder, RemoveVolumeOptionsBuilder};

    if let Err(error) = docker
        .remove_container(
            name,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await
    {
        if !is_not_found(&error) {
            eprintln!("failed to clean up BuildKit container `{name}`: {error}");
        }
    }
    if let Err(error) = docker
        .remove_volume(
            volume_name,
            Some(RemoveVolumeOptionsBuilder::default().build()),
        )
        .await
    {
        if !is_not_found(&error) {
            eprintln!("failed to clean up BuildKit volume `{volume_name}`: {error}");
        }
    }
}

async fn export_buildkit_oci_test(docker: Docker) -> Result<(), Error> {
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

    let frontend_opts = bollard::grpc::build::ImageBuildFrontendOptions::builder()
        .pull(true)
        .build();

    let dest_path = std::path::Path::new("/tmp/oci-image.tar");

    // cleanup - usually for local testing, the grpc handler will overwrite
    if dest_path.exists() {
        std::fs::remove_file(dest_path).unwrap();
    }
    assert!(!dest_path.exists());

    let output = bollard::grpc::export::ImageExporterOutputBuilder::new(
        "docker.io/library/bollard-oci-export-buildkit-example:latest",
    )
    .annotation("exporter", "Bollard")
    .dest(dest_path);

    let name = unique_builder_name("bollard_phase_e_oci");
    let volume_name = format!("{name}_state");
    let result = async {
        let mut buildkit_builder = DockerContainerBuilder::new(&docker);
        buildkit_builder
            .name(&name)
            .lifecycle(DockerContainerLifecycle::RemoveAfterSolve { keep_state: false });
        let driver = buildkit_builder
            .bootstrap()
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("BuildKit bootstrap failed: {error}")),
            })?;

        let load_input =
            bollard::grpc::build::ImageBuildLoadInput::Upload(bytes::Bytes::from(compressed));

        let credentials = bollard::auth::DockerCredentials {
            username: Some("bollard".to_string()),
            password: std::env::var("REGISTRY_PASSWORD").ok(),
            ..Default::default()
        };
        let mut creds_hsh = std::collections::HashMap::new();
        creds_hsh.insert("localhost:5000", credentials);

        bollard::grpc::driver::Export::export(
            &driver,
            bollard::grpc::driver::ImageExporterEnum::OCI(output),
            frontend_opts,
            load_input,
            Some(creds_hsh),
            None,
        )
        .await
        .map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("BuildKit solve failed: {error}")),
        })?;

        assert!(dest_path.exists());

        let oci_file = std::fs::File::open(dest_path)?;
        let mut oci_archive = tar::Archive::new(oci_file);

        let mut paths = vec![];
        let iter = oci_archive.entries()?;
        for entry in iter {
            let entry = entry?;
            let path = entry.path()?.display().to_string();
            paths.push(path);
        }

        println!("{:#?}", paths);

        assert!(paths.contains(&String::from("blobs/")));
        assert!(paths.contains(&String::from("blobs/sha256/")));
        assert!(paths.contains(&String::from("index.json")));
        assert!(paths.contains(&String::from("oci-layout")));
        assert_eq!(paths.len(), 8);
        Ok::<(), Error>(())
    }
    .await;

    cleanup_named_builder(&docker, &name, &volume_name).await;
    let _ = std::fs::remove_file(dest_path);
    result
}

#[cfg(feature = "buildkit_providerless")]
async fn persistent_builder_multi_solve_test(docker: Docker) -> Result<(), Error> {
    let name = unique_builder_name("bollard_phase_e_multi");
    let volume_name = format!("{name}_state");
    let first_output = std::path::PathBuf::from(format!("/tmp/{name}_first.tar"));
    let second_output = std::path::PathBuf::from(format!("/tmp/{name}_second.tar"));

    let result = async {
        let dockerfile = b"FROM scratch\n";
        let mut header = tar::Header::new_gnu();
        header.set_path("Dockerfile").unwrap();
        header.set_size(dockerfile.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        let mut tar = tar::Builder::new(Vec::new());
        tar.append(&header, dockerfile.as_slice()).unwrap();
        let uncompressed = tar.into_inner().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&uncompressed).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut first_builder = DockerContainerBuilder::new(&docker);
        first_builder.name(&name);
        let first = first_builder.bootstrap().await.unwrap();
        let first_inspect = docker.inspect_container(first.name(), None).await?;
        let first_id = first_inspect.id.clone();

        for output_path in [&first_output, &second_output] {
            let output = bollard::grpc::export::ImageExporterOutputBuilder::new(
                "bollard-phase-c-scratch:latest",
            )
            .dest(output_path);
            bollard::grpc::driver::Export::export(
                &first,
                bollard::grpc::driver::ImageExporterEnum::OCI(output),
                bollard::grpc::build::ImageBuildFrontendOptions::default(),
                bollard::grpc::build::ImageBuildLoadInput::Upload(bytes::Bytes::from(
                    compressed.clone(),
                )),
                None,
                None,
            )
            .await
            .unwrap();
            assert!(output_path.exists());
        }

        let mut second_builder = DockerContainerBuilder::new(&docker);
        second_builder.name(&name);
        let second = second_builder.bootstrap().await.unwrap();
        let second_inspect = docker.inspect_container(second.name(), None).await?;

        assert_eq!(first_id, second_inspect.id);
        assert_eq!(docker.inspect_volume(&volume_name).await?.name, volume_name);
        Ok::<(), Error>(())
    }
    .await;

    cleanup_named_builder(&docker, &name, &volume_name).await;
    let _ = std::fs::remove_file(&first_output);
    let _ = std::fs::remove_file(&second_output);
    result
}

#[cfg(feature = "buildkit_providerless")]
fn oci_manifest_digest(path: &std::path::Path) -> Result<String, Error> {
    use std::io::Read;

    let file = std::fs::File::open(path)?;
    let mut archive = tar::Archive::new(file);
    let mut index = String::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.to_string_lossy() == "index.json" {
            entry.read_to_string(&mut index)?;
            break;
        }
    }

    let index: serde_json::Value = serde_json::from_str(&index)?;
    index
        .get("manifests")
        .and_then(serde_json::Value::as_array)
        .and_then(|manifests| manifests.first())
        .and_then(|manifest| manifest.get("digest"))
        .and_then(serde_json::Value::as_str)
        .map(String::from)
        .ok_or_else(|| Error::IOError {
            err: std::io::Error::other("OCI index does not contain a manifest digest"),
        })
}

#[cfg(feature = "buildkit_providerless")]
async fn persistent_builder_cache_repeatability_test(docker: Docker) -> Result<(), Error> {
    use bollard::grpc::build::{ImageBuildFrontendOptions, ImageBuildLoadInput};
    use bollard::grpc::driver::{Export, ImageExporterEnum};
    use bollard::grpc::export::ImageExporterOutputBuilder;

    let name = unique_builder_name("bollard_phase_e_cache");
    let volume_name = format!("{name}_state");
    let output_root = tempfile::tempdir()?;
    let first_output = output_root.path().join("first.tar");
    let second_output = output_root.path().join("second.tar");
    let dockerfile = "FROM localhost:5000/alpine\nRUN date +%s > /cache-proof\nFROM scratch\nCOPY --from=0 /cache-proof /cache-proof\n";

    let result = async {
        let mut header = tar::Header::new_gnu();
        header.set_path("Dockerfile").unwrap();
        header.set_size(dockerfile.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        let mut tar = tar::Builder::new(Vec::new());
        tar.append(&header, dockerfile.as_bytes()).unwrap();
        let uncompressed = tar.into_inner().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&uncompressed).unwrap();
        let input = bytes::Bytes::from(encoder.finish().unwrap());

        let mut first_builder = DockerContainerBuilder::new(&docker);
        first_builder.name(&name);
        let first = first_builder
            .bootstrap()
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("first BuildKit bootstrap failed: {error}")),
            })?;
        let first_id = docker
            .inspect_container(first.name(), None)
            .await?
            .id
            .expect("bootstrapped container has an ID");

        let credentials = bollard::auth::DockerCredentials {
            username: Some("bollard".to_string()),
            password: std::env::var("REGISTRY_PASSWORD").ok(),
            ..Default::default()
        };
        let mut credentials_by_host = std::collections::HashMap::new();
        credentials_by_host.insert("localhost:5000", credentials);
        Export::export(
            &first,
            ImageExporterEnum::OCI(
                ImageExporterOutputBuilder::new("bollard-phase-e-cache:latest").dest(&first_output),
            ),
            ImageBuildFrontendOptions::builder().pull(true).build(),
            ImageBuildLoadInput::Upload(input.clone()),
            Some(credentials_by_host),
            None,
        )
        .await
        .map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("first BuildKit solve failed: {error}")),
        })?;
        let first_digest = oci_manifest_digest(&first_output)?;

        first
            .remove(DockerContainerRemoveOptions::new().keep_state(true))
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("failed to remove first builder: {error}")),
            })?;
        assert!(docker.inspect_container(&name, None).await.is_err());
        assert_eq!(docker.inspect_volume(&volume_name).await?.name, volume_name);

        // If the RUN step executes again, its timestamp and resulting layer digest change.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let mut second_builder = DockerContainerBuilder::new(&docker);
        second_builder.name(&name);
        let second = second_builder
            .bootstrap()
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("second BuildKit bootstrap failed: {error}")),
            })?;
        let second_id = docker
            .inspect_container(second.name(), None)
            .await?
            .id
            .expect("rebootstrapped container has an ID");
        assert_ne!(first_id, second_id);

        let credentials = bollard::auth::DockerCredentials {
            username: Some("bollard".to_string()),
            password: std::env::var("REGISTRY_PASSWORD").ok(),
            ..Default::default()
        };
        let mut credentials_by_host = std::collections::HashMap::new();
        credentials_by_host.insert("localhost:5000", credentials);
        Export::export(
            &second,
            ImageExporterEnum::OCI(
                ImageExporterOutputBuilder::new("bollard-phase-e-cache:latest")
                    .dest(&second_output),
            ),
            ImageBuildFrontendOptions::builder().pull(true).build(),
            ImageBuildLoadInput::Upload(input),
            Some(credentials_by_host),
            None,
        )
        .await
        .map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("second BuildKit solve failed: {error}")),
        })?;
        let second_digest = oci_manifest_digest(&second_output)?;
        assert_eq!(first_digest, second_digest);

        second
            .remove(DockerContainerRemoveOptions::default())
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("failed to remove second builder: {error}")),
            })?;
        assert!(docker.inspect_volume(&volume_name).await.is_err());
        Ok::<(), Error>(())
    }
    .await;

    cleanup_named_builder(&docker, &name, &volume_name).await;
    result
}

#[cfg(feature = "buildkit_providerless")]
async fn persistent_builder_management_test(docker: Docker) -> Result<(), Error> {
    let name = unique_builder_name("bollard_phase_e_management");
    let volume_name = format!("{name}_state");

    let result = async {
        let mut first_builder = DockerContainerBuilder::new(&docker);
        first_builder.name(&name);
        let first = first_builder.bootstrap().await.unwrap();
        let first_id = docker
            .inspect_container(first.name(), None)
            .await?
            .id
            .expect("bootstrapped container has an ID");

        first.stop().await.unwrap();
        first.stop().await.unwrap();
        let stopped = docker.inspect_container(first.name(), None).await?;
        assert_eq!(
            stopped.state.and_then(|state| state.status),
            Some(bollard::models::ContainerStateStatusEnum::EXITED)
        );

        let mut second_builder = DockerContainerBuilder::new(&docker);
        second_builder.name(&name);
        let second = second_builder.bootstrap().await.unwrap();
        let second_id = docker
            .inspect_container(second.name(), None)
            .await?
            .id
            .expect("reused container has an ID");
        assert_eq!(first_id, second_id);

        second
            .remove(DockerContainerRemoveOptions::new().keep_state(true))
            .await
            .unwrap();
        assert!(docker.inspect_container(&name, None).await.is_err());
        assert_eq!(docker.inspect_volume(&volume_name).await?.name, volume_name);

        let mut third_builder = DockerContainerBuilder::new(&docker);
        third_builder.name(&name);
        let third = third_builder.bootstrap().await.unwrap();
        let third_id = docker
            .inspect_container(third.name(), None)
            .await?
            .id
            .expect("recreated container has an ID");
        assert_ne!(first_id, third_id);

        third
            .remove(DockerContainerRemoveOptions::new())
            .await
            .unwrap();
        third
            .remove(DockerContainerRemoveOptions::new())
            .await
            .unwrap();
        assert!(docker.inspect_container(&name, None).await.is_err());
        assert!(docker.inspect_volume(&volume_name).await.is_err());
        Ok::<(), Error>(())
    }
    .await;

    cleanup_named_builder(&docker, &name, &volume_name).await;
    result
}

#[cfg(feature = "buildkit_providerless")]
async fn ephemeral_builder_cleanup_test(docker: Docker) -> Result<(), Error> {
    use bollard::grpc::build::{ImageBuildFrontendOptions, ImageBuildLoadInput};
    use bollard::grpc::driver::{Export, ImageExporterEnum};
    use bollard::grpc::export::ImageExporterOutputBuilder;
    let stop_name = unique_builder_name("bollard_phase_e_stop");
    let remove_name = unique_builder_name("bollard_phase_e_remove");
    let keep_name = unique_builder_name("bollard_phase_e_keep");
    let stop_volume = format!("{stop_name}_state");
    let remove_volume = format!("{remove_name}_state");
    let keep_volume = format!("{keep_name}_state");
    let stop_output = std::path::PathBuf::from(format!("/tmp/{stop_name}.tar"));
    let remove_output = std::path::PathBuf::from(format!("/tmp/{remove_name}.tar"));
    let keep_output = std::path::PathBuf::from(format!("/tmp/{keep_name}.tar"));

    let result = async {
        let dockerfile = b"FROM scratch\n";
        let mut header = tar::Header::new_gnu();
        header.set_path("Dockerfile").unwrap();
        header.set_size(dockerfile.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        let mut tar = tar::Builder::new(Vec::new());
        tar.append(&header, dockerfile.as_slice()).unwrap();
        let uncompressed = tar.into_inner().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&uncompressed).unwrap();
        let input = bytes::Bytes::from(encoder.finish().unwrap());

        let mut stop_builder = DockerContainerBuilder::new(&docker);
        stop_builder
            .name(&stop_name)
            .lifecycle(DockerContainerLifecycle::StopAfterSolve);
        let stop_driver = stop_builder.bootstrap().await.unwrap();
        Export::export(
            &stop_driver,
            ImageExporterEnum::OCI(
                ImageExporterOutputBuilder::new("bollard-phase-d-stop:latest").dest(&stop_output),
            ),
            ImageBuildFrontendOptions::default(),
            ImageBuildLoadInput::Upload(input.clone()),
            None,
            None,
        )
        .await
        .unwrap();
        let stopped = docker.inspect_container(&stop_name, None).await?;
        assert_eq!(
            stopped.state.and_then(|state| state.status),
            Some(bollard::models::ContainerStateStatusEnum::EXITED)
        );
        assert_eq!(docker.inspect_volume(&stop_volume).await?.name, stop_volume);

        let mut remove_builder = DockerContainerBuilder::new(&docker);
        remove_builder
            .name(&remove_name)
            .lifecycle(DockerContainerLifecycle::RemoveAfterSolve { keep_state: false });
        let remove_driver = remove_builder.bootstrap().await.unwrap();
        Export::export(
            &remove_driver,
            ImageExporterEnum::OCI(
                ImageExporterOutputBuilder::new("bollard-phase-d-remove:latest")
                    .dest(&remove_output),
            ),
            ImageBuildFrontendOptions::default(),
            ImageBuildLoadInput::Upload(input.clone()),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(docker.inspect_container(&remove_name, None).await.is_err());
        assert!(docker.inspect_volume(&remove_volume).await.is_err());

        let mut keep_builder = DockerContainerBuilder::new(&docker);
        keep_builder
            .name(&keep_name)
            .lifecycle(DockerContainerLifecycle::RemoveAfterSolve { keep_state: true });
        let keep_driver = keep_builder.bootstrap().await.unwrap();
        Export::export(
            &keep_driver,
            ImageExporterEnum::OCI(
                ImageExporterOutputBuilder::new("bollard-phase-d-keep:latest").dest(&keep_output),
            ),
            ImageBuildFrontendOptions::default(),
            ImageBuildLoadInput::Upload(input),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(docker.inspect_container(&keep_name, None).await.is_err());
        assert_eq!(docker.inspect_volume(&keep_volume).await?.name, keep_volume);
        keep_driver
            .remove(DockerContainerRemoveOptions::default())
            .await
            .unwrap();
        assert!(docker.inspect_volume(&keep_volume).await.is_err());
        Ok::<(), Error>(())
    }
    .await;

    for (name, volume) in [
        (&stop_name, &stop_volume),
        (&remove_name, &remove_volume),
        (&keep_name, &keep_volume),
    ] {
        cleanup_named_builder(&docker, name, volume).await;
    }
    let _ = std::fs::remove_file(&stop_output);
    let _ = std::fs::remove_file(&remove_output);
    let _ = std::fs::remove_file(&keep_output);
    result
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_export_buildkit_oci() {
    connect_to_docker_and_run!(export_buildkit_oci_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_persistent_builder_multi_solve() {
    connect_to_docker_and_run!(persistent_builder_multi_solve_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_persistent_builder_management() {
    connect_to_docker_and_run!(persistent_builder_management_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_persistent_builder_cache_repeatability() {
    connect_to_docker_and_run!(persistent_builder_cache_repeatability_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_ephemeral_builder_cleanup() {
    connect_to_docker_and_run!(ephemeral_builder_cleanup_test);
}
