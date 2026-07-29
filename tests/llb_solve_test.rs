#![cfg(feature = "buildkit")]

use bollard::errors::Error;
use bollard::grpc::driver::docker_container::DockerContainerBuilder;
use bollard::grpc::driver::{DefinitionExporter, DefinitionSolveRequest, SolveDefinition};
use bollard::Docker;
use bollard_buildkit_proto::pb;
use prost::Message;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;

#[macro_use]
pub mod common;
use crate::common::*;

const MINIMAL_MKFILE_DEFINITION_HEX: &str = concat!(
    "0a3b5a002237123508ffffffffffffffffff0110ffffffffffffffffff01",
    "2a1d0a062f68656c6c6f10a4031a05776f726c6428ffffffffffffffffff",
    "010a4b0a490a477368613235363a63636433313636376132333330393365",
    "333431633432393437336261316264393635653433353535343361663630",
    "613932343562303535363163656265623066126a0a477368613235363a62",
    "323437643630373064333566646134303730616535343261653235656633",
    "626637313835326264633163653635613431366133653764393334363834",
    "623361121f2a0c0a08706c6174666f726d10012a0f0a0b636f6e73747261",
    "696e74731001125a0a477368613235363a63636433313636376132333330",
    "393365333431633432393437336261316264393635653433353535343361",
    "663630613932343562303535363163656265623066120f2a0d0a0966696c",
    "652e6261736510011a4d0a4b0a477368613235363a636364333136363761",
    "323333303933653334316334323934373362613162643936356534333535",
    "353433616636306139323435623035353631636562656230661200"
);

fn minimal_mkfile_definition() -> pb::Definition {
    let bytes = (0..MINIMAL_MKFILE_DEFINITION_HEX.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&MINIMAL_MKFILE_DEFINITION_HEX[index..index + 2], 16)
                .expect("minimal definition hex is valid")
        })
        .collect::<Vec<_>>();
    pb::Definition::decode(bytes.as_slice()).expect("minimal definition is valid protobuf")
}

fn local_source_definition() -> pb::Definition {
    let source_operation = pb::Op {
        op: Some(pb::op::Op::Source(pb::SourceOp {
            identifier: String::from("local://context"),
            attrs: Default::default(),
        })),
        ..Default::default()
    };
    let source_bytes = source_operation.encode_to_vec();
    let source_digest = format!("sha256:{:x}", Sha256::digest(&source_bytes));
    let copy = pb::FileActionCopy {
        src: String::from("/"),
        dest: String::from("/"),
        owner: None,
        mode: -1,
        follow_symlink: false,
        dir_copy_contents: true,
        attempt_unpack_docker_compatibility: false,
        create_dest_path: true,
        allow_wildcard: false,
        allow_empty_wildcard: false,
        timestamp: -1,
        include_patterns: Vec::new(),
        exclude_patterns: Vec::new(),
        always_replace_existing_dest_paths: false,
        mode_str: String::new(),
        required_paths: Vec::new(),
    };
    let root_operation = pb::Op {
        inputs: vec![pb::Input {
            digest: source_digest,
            index: 0,
        }],
        op: Some(pb::op::Op::File(pb::FileOp {
            actions: vec![pb::FileAction {
                input: -1,
                secondary_input: 0,
                output: 0,
                action: Some(pb::file_action::Action::Copy(copy)),
            }],
        })),
        ..Default::default()
    };
    let root_bytes = root_operation.encode_to_vec();
    let wrapper_operation = pb::Op {
        inputs: vec![pb::Input {
            digest: format!("sha256:{:x}", Sha256::digest(&root_bytes)),
            index: 0,
        }],
        ..Default::default()
    };
    let wrapper_bytes = wrapper_operation.encode_to_vec();

    pb::Definition {
        def: vec![source_bytes, root_bytes, wrapper_bytes],
        metadata: Default::default(),
        source: None,
    }
}

fn unique_builder_name() -> String {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_nanos();
    format!("bollard_llb_gate_e_{suffix}")
}

async fn direct_definition_solve_test(docker: Docker) -> Result<(), Error> {
    let name = unique_builder_name();
    let volume_name = format!("{name}_state");
    let first_output = tempfile::tempdir()?;
    let second_output = tempfile::tempdir()?;

    let result = async {
        let mut builder = DockerContainerBuilder::new(&docker);
        builder.name(&name);
        let driver = builder.bootstrap().await.map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("BuildKit bootstrap failed: {error}")),
        })?;

        let definition = minimal_mkfile_definition();
        for output in [first_output.path(), second_output.path()] {
            let request = DefinitionSolveRequest::new(
                definition.clone(),
                DefinitionExporter::Local(output.to_path_buf()),
            );
            SolveDefinition::solve_definition(&driver, request)
                .await
                .map_err(|error| Error::IOError {
                    err: std::io::Error::other(format!("direct definition solve failed: {error}")),
                })?;

            assert_eq!(std::fs::read(output.join("hello"))?, b"world");
        }

        let container = docker.inspect_container(driver.name(), None).await?;
        let expected_container_name = format!("/{name}");
        assert_eq!(
            container.name.as_deref(),
            Some(expected_container_name.as_str())
        );
        assert_eq!(docker.inspect_volume(&volume_name).await?.name, volume_name);
        Ok::<(), Error>(())
    }
    .await;

    let _ = driver_cleanup(&docker, &name, &volume_name).await;
    result
}

async fn local_source_filesync_solve_test(docker: Docker) -> Result<(), Error> {
    let name = unique_builder_name();
    let volume_name = format!("{name}_state");
    let source = tempfile::tempdir()?;
    let output = tempfile::tempdir()?;
    std::fs::create_dir(source.path().join("nested"))?;
    std::fs::write(source.path().join("nested/input.txt"), b"local source")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink("nested/input.txt", source.path().join("link"))?;

    let result = async {
        let mut builder = DockerContainerBuilder::new(&docker);
        builder.name(&name);
        let driver = builder.bootstrap().await.map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("BuildKit bootstrap failed: {error}")),
        })?;
        let options = bollard::grpc::driver::DefinitionSolveOptionsBuilder::new()
            .local_mount("context", source.path())
            .build();
        let request = DefinitionSolveRequest::new(
            local_source_definition(),
            DefinitionExporter::Local(output.path().to_path_buf()),
        )
        .with_options(options);
        SolveDefinition::solve_definition(&driver, request)
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("local FileSync solve failed: {error}")),
            })?;

        assert_eq!(
            std::fs::read(output.path().join("nested/input.txt"))?,
            b"local source"
        );
        #[cfg(unix)]
        assert_eq!(
            std::fs::read_link(output.path().join("link"))?,
            std::path::Path::new("nested/input.txt")
        );
        Ok::<(), Error>(())
    }
    .await;

    let _ = driver_cleanup(&docker, &name, &volume_name).await;
    result
}

async fn driver_cleanup(docker: &Docker, name: &str, volume_name: &str) -> Result<(), Error> {
    use bollard::query_parameters::{RemoveContainerOptionsBuilder, RemoveVolumeOptionsBuilder};

    let _ = docker
        .remove_container(
            name,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await;
    let _ = docker
        .remove_volume(
            volume_name,
            Some(RemoveVolumeOptionsBuilder::default().build()),
        )
        .await;
    Ok(())
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_direct_definition_solve() {
    connect_to_docker_and_run!(direct_definition_solve_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_local_source_filesync_solve() {
    connect_to_docker_and_run!(local_source_filesync_solve_test);
}

#[test]
fn direct_definition_request_accepts_known_valid_protobuf() {
    let request = DefinitionSolveRequest::new(
        minimal_mkfile_definition(),
        DefinitionExporter::Local(std::path::PathBuf::from("/tmp/output")),
    );
    assert_eq!(request.definition.def.len(), 2);
}
