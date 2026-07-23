#![cfg(feature = "buildkit")]

use bollard::errors::Error;
use bollard::Docker;

use bollard::grpc::driver::docker_container::DockerContainerBuilder;
use tokio::runtime::Runtime;

use std::io::Write;

#[macro_use]
pub mod common;
use crate::common::*;

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

    let buildkit_builder = DockerContainerBuilder::new(&docker);
    let driver = buildkit_builder.bootstrap().await.unwrap();

    let load_input =
        bollard::grpc::build::ImageBuildLoadInput::Upload(bytes::Bytes::from(compressed));

    let credentials = bollard::auth::DockerCredentials {
        username: Some("bollard".to_string()),
        password: std::env::var("REGISTRY_PASSWORD").ok(),
        ..Default::default()
    };
    let mut creds_hsh = std::collections::HashMap::new();
    creds_hsh.insert("localhost:5000", credentials);

    let res = bollard::grpc::driver::Export::export(
        &driver,
        bollard::grpc::driver::ImageExporterEnum::OCI(output),
        frontend_opts,
        load_input,
        Some(creds_hsh),
        None,
    )
    .await;

    assert!(res.is_ok());

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

    println!("{:#?}", &paths);

    assert!(paths.contains(&String::from("blobs/")));
    assert!(paths.contains(&String::from("blobs/sha256/")));
    assert!(paths.contains(&String::from("index.json")));
    assert!(paths.contains(&String::from("oci-layout")));

    assert_eq!(paths.len(), 8);

    Ok(())
}

#[cfg(feature = "buildkit_providerless")]
async fn persistent_builder_multi_solve_test(docker: Docker) -> Result<(), Error> {
    use bollard::query_parameters::{RemoveContainerOptionsBuilder, RemoveVolumeOptions};
    use std::time::{SystemTime, UNIX_EPOCH};

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_nanos();
    let name = format!("bollard_phase_b_{suffix}");
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

    let _ = docker
        .remove_container(
            &name,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await;
    let _ = docker
        .remove_volume(&volume_name, None::<RemoveVolumeOptions>)
        .await;
    let _ = std::fs::remove_file(&first_output);
    let _ = std::fs::remove_file(&second_output);
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
