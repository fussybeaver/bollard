//! Builds and runs a Rust application as a configured scratch image through Moby BuildKit.

use std::path::PathBuf;

use bollard::grpc::build::ImageBuildPlatform;
use bollard::grpc::driver::{moby::Moby, DefinitionSolveOptionsBuilder, MobyImageBuildRequest};
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::RemoveContainerOptionsBuilder;
use bollard::Docker;
use bollard_llb as llb;
use futures_util::TryStreamExt;

const IMAGE_NAME: &str = "bollard-llb-app-example";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_socket_defaults()?;
    let context_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/fixtures/rust-app");
    let definition = app_definition()?;
    let driver = Moby::new(&docker);
    let options = DefinitionSolveOptionsBuilder::new()
        .local_mount("context", &context_path)?
        .build();
    let image_tag = format!("{IMAGE_NAME}:{}", std::process::id());
    let image_config = serde_json::json!({
        "architecture": "amd64",
        "os": "linux",
        "config": {
            "Entrypoint": ["/hello"],
            "Cmd": [],
        }
    });

    driver
        .build_image(
            MobyImageBuildRequest::new(
                definition.to_pb(),
                image_config,
                ImageBuildPlatform {
                    architecture: "amd64".into(),
                    os: "linux".into(),
                    variant: None,
                },
                image_tag.clone(),
            )
            .with_options(options),
        )
        .await?;

    println!("Built configured image directly into Docker: {image_tag}");

    let container = docker
        .create_container(
            None::<bollard::query_parameters::CreateContainerOptions>,
            ContainerCreateBody {
                image: Some(image_tag.clone()),
                // No command or entrypoint override: Docker uses the stored image config.
                ..Default::default()
            },
        )
        .await?;
    let run_result = async {
        docker
            .start_container(
                &container.id,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await?;
        let result = docker
            .wait_container(
                &container.id,
                None::<bollard::query_parameters::WaitContainerOptions>,
            )
            .try_next()
            .await?
            .ok_or("container exited without a wait result")?;
        if result.status_code != 0 {
            return Err(format!("container exited with status {}", result.status_code).into());
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

    println!("Verified runnable configured image {image_tag}");
    Ok(())
}

fn app_definition() -> Result<llb::Definition, Box<dyn std::error::Error>> {
    let context: llb::State = llb::Local::new("context")?
        .with_include_patterns(["Cargo.toml", "src/**"])?
        .into();

    // Raw LLB exec metadata does not inherit image ENV automatically.
    let builder = llb::image("rust:1.86-alpine")?
        .add_env("CARGO_HOME", "/usr/local/cargo")
        .add_env("RUSTUP_HOME", "/usr/local/rustup")
        .add_env(
            "PATH",
            "/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .file(
            llb::copy(context, "/", "/app")
                .with_copy_dir_contents_only(true)
                .with_create_dest_path(true),
            llb::FileOpts::new().with_custom_name("copy Rust project into builder"),
        )?;

    let compiled = builder
        .dir("/app")
        .run(llb::shlex("cargo build --release")?)
        .with_custom_name("compile Rust application")
        .add_mount_cache(
            "/usr/local/cargo/registry",
            "rust-app-registry",
            llb::CacheSharingMode::Shared,
        )
        .root()?;
    let artifact = llb::scratch()?.file(
        llb::copy(compiled, "/app/target/release/hello", "/hello"),
        llb::FileOpts::new().with_custom_name("select application artifact"),
    )?;
    Ok(artifact.marshal(llb::MarshalOpts::linux_amd64())?)
}
