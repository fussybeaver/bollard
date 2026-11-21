//! Patches and runs an Alpine image using a direct LLB solve.

use bollard::grpc::driver::docker_container::{DockerContainerBuilder, DockerContainerLifecycle};
use bollard::grpc::driver::{DefinitionExporter, DefinitionSolveRequest, SolveDefinition};
use bollard::grpc::export::ImageExporterOutput;
use bollard::llb::{self, CacheSharingMode, Image, MarshalOpts, RunOpts};
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptionsBuilder, RemoveContainerOptionsBuilder,
    StartContainerOptions, WaitContainerOptions,
};
use bollard::Docker;
use futures_util::TryStreamExt;

const BASE_IMAGE: &str = "alpine:3.22";
const IMAGE_NAME: &str = "bollard-llb-patched-example";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_socket_defaults()?;
    docker
        .create_image(
            Some(
                CreateImageOptionsBuilder::default()
                    .from_image(BASE_IMAGE)
                    .build(),
            ),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await?;
    let base_image = docker
        .inspect_image(
            BASE_IMAGE,
            None::<bollard::query_parameters::InspectImageOptions>,
        )
        .await?;
    let base_config = base_image
        .config
        .clone()
        .ok_or("base image has no image config")?;
    let image_config = serde_json::json!({
        "os": base_image.os.as_deref().ok_or("base image has no OS")?,
        "architecture": base_image
            .architecture
            .as_deref()
            .ok_or("base image has no architecture")?,
        "config": base_config,
    });

    let temp = tempfile::tempdir()?;
    let archive_path = temp.path().join("patched-image.tar");
    let image_tag = format!("{IMAGE_NAME}:{}", std::process::id());
    let definition = patch_definition()?;

    let export = ImageExporterOutput::builder(&image_tag)
        .image_config_json(serde_json::to_string(&image_config)?)
        .dest(&archive_path);
    let mut builder = DockerContainerBuilder::new(&docker);
    builder.lifecycle(DockerContainerLifecycle::RemoveAfterSolve { keep_state: false });
    let buildkit = builder.bootstrap().await?;
    buildkit
        .solve_definition(DefinitionSolveRequest::new(
            definition.to_pb(),
            DefinitionExporter::Docker(export),
        ))
        .await?;

    let archive = tokio::fs::read(&archive_path).await?;
    docker
        .import_image(
            bollard::query_parameters::ImportImageOptionsBuilder::default().build(),
            bollard::body_full(bytes::Bytes::from(archive)),
            None,
        )
        .try_collect::<Vec<_>>()
        .await?;

    println!("Loaded patched image {image_tag}");

    let container = docker
        .create_container(
            None::<CreateContainerOptions>,
            ContainerCreateBody {
                image: Some(image_tag.clone()),
                cmd: Some(vec![
                    "sh".into(),
                    "-c".into(),
                    "test -f /etc/alpine-release && apk info -e busybox && echo PATCHED_IMAGE_OK"
                        .into(),
                ]),
                ..Default::default()
            },
        )
        .await?;
    let run_result = async {
        docker
            .start_container(&container.id, None::<StartContainerOptions>)
            .await?;
        let result = docker
            .wait_container(&container.id, None::<WaitContainerOptions>)
            .try_next()
            .await?
            .ok_or("container exited without a wait result")?;
        if result.status_code != 0 {
            return Err(format!(
                "patched-image container exited with status {}",
                result.status_code
            )
            .into());
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    let cleanup = docker
        .remove_container(
            &container.id,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await;
    run_result?;
    cleanup?;

    println!("Verified running patched image {image_tag}");
    Ok(())
}

fn patch_definition() -> Result<llb::Definition, Box<dyn std::error::Error>> {
    let base: bollard::llb::State = Image::new(BASE_IMAGE)?.into();
    let upgraded = base
        .run(
            RunOpts::new()
                .with_arg("sh")
                .with_arg("-c")
                .with_arg("apk update && apk upgrade")
                // Force this network-dependent step to rerun. Package versions
                // can change over time, so this is intentionally not reproducible.
                .with_ignore_cache(true),
        )
        .with_custom_name("upgrade installed Alpine packages")
        .add_mount_cache("/var/cache/apk", "alpine-apk", CacheSharingMode::Shared)
        .root()?;
    Ok(upgraded.marshal(MarshalOpts::linux_amd64())?)
}
