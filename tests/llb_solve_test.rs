#![cfg(feature = "buildkit")]

use bollard::container::LogOutput;
use bollard::errors::Error;
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::grpc::driver::docker_container::DockerContainerBuilder;
use bollard::grpc::driver::{
    DefinitionExporter, DefinitionSolveOptionsBuilder, DefinitionSolveRequest, SolveDefinition,
};
use bollard::Docker;
use bollard_buildkit_proto::pb;
use futures_util::TryStreamExt;
use prost::Message;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

fn operation_digest(operation: &pb::Op) -> String {
    format!("sha256:{:x}", Sha256::digest(operation.encode_to_vec()))
}

fn source_operation(identifier: impl Into<String>) -> (Vec<u8>, String) {
    let identifier = identifier.into();
    let attrs = if identifier.starts_with("docker-image://") {
        BTreeMap::from([(String::from("image.resolvemode"), String::from("default"))])
    } else {
        BTreeMap::new()
    };
    let operation = pb::Op {
        op: Some(pb::op::Op::Source(pb::SourceOp { identifier, attrs })),
        ..Default::default()
    };
    let bytes = operation.encode_to_vec();
    let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
    (bytes, digest)
}

fn local_source_copy_definition(name: &str, destination: &str) -> pb::Definition {
    local_source_copy_definition_with_patterns(name, destination, &[], &[])
}

fn local_source_copy_definition_with_patterns(
    name: &str,
    destination: &str,
    include_patterns: &[&str],
    exclude_patterns: &[&str],
) -> pb::Definition {
    let (source_bytes, source_digest) = source_operation(format!("local://{name}"));
    let copy = pb::FileActionCopy {
        src: String::from("/"),
        dest: String::from(destination),
        owner: None,
        mode: -1,
        follow_symlink: false,
        dir_copy_contents: true,
        attempt_unpack_docker_compatibility: false,
        create_dest_path: true,
        allow_wildcard: false,
        allow_empty_wildcard: false,
        timestamp: -1,
        include_patterns: include_patterns
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        exclude_patterns: exclude_patterns
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
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

    pb::Definition {
        def: vec![source_bytes, root_bytes, wrapper_operation.encode_to_vec()],
        metadata: Default::default(),
        source: None,
    }
}

fn local_source_exec_definition(image_identifier: &str, name: &str) -> pb::Definition {
    let (local_bytes, local_digest) = source_operation(format!("local://{name}"));
    let (image_bytes, image_digest) = source_operation(image_identifier);
    let exec_operation = pb::Op {
        inputs: vec![
            pb::Input {
                digest: image_digest,
                index: 0,
            },
            pb::Input {
                digest: local_digest,
                index: 0,
            },
        ],
        op: Some(pb::op::Op::Exec(pb::ExecOp {
            meta: Some(pb::Meta {
                args: vec![
                    String::from("/bin/sh"),
                    String::from("-c"),
                    String::from(
                        "cat /app/nested/input.txt > /result.txt; date +%s > /cache-proof",
                    ),
                ],
                cwd: String::from("/"),
                ..Default::default()
            }),
            mounts: vec![
                pb::Mount {
                    input: 0,
                    dest: String::from("/"),
                    output: 0,
                    mount_type: pb::MountType::Bind as i32,
                    ..Default::default()
                },
                pb::Mount {
                    input: 1,
                    dest: String::from("/app"),
                    output: -1,
                    readonly: true,
                    mount_type: pb::MountType::Bind as i32,
                    ..Default::default()
                },
            ],
            ..Default::default()
        })),
        ..Default::default()
    };
    let wrapper_operation = pb::Op {
        inputs: vec![pb::Input {
            digest: operation_digest(&exec_operation),
            index: 0,
        }],
        ..Default::default()
    };

    pb::Definition {
        def: vec![
            local_bytes,
            image_bytes,
            exec_operation.encode_to_vec(),
            wrapper_operation.encode_to_vec(),
        ],
        metadata: Default::default(),
        source: None,
    }
}

fn alpine_image_reference() -> (String, Option<String>) {
    if std::env::var_os("DISABLE_REGISTRY").is_some() {
        return (String::from("docker.io/library/alpine:latest"), None);
    }

    let host = std::env::var("REGISTRY_HTTP_ADDR")
        .unwrap_or_else(|_| String::from("localhost:5000"))
        .trim_end_matches('/')
        .to_string();
    (format!("{host}/alpine:latest"), Some(host))
}

fn local_source_options(
    source_name: &str,
    source_path: &Path,
    image_registry: Option<&str>,
) -> Result<bollard::grpc::driver::DefinitionSolveOptions, Error> {
    let mut builder = DefinitionSolveOptionsBuilder::new()
        .local_mount(source_name, source_path)
        .map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("local mount setup failed: {error}")),
        })?;
    if let Some(host) = image_registry {
        builder = builder.credential(host, integration_test_registry_credentials());
    }
    Ok(builder.build())
}

fn create_application_fixture() -> Result<tempfile::TempDir, Error> {
    let source = tempfile::tempdir()?;
    std::fs::create_dir(source.path().join("nested"))?;
    std::fs::write(source.path().join("nested/input.txt"), b"local source")?;
    #[cfg(unix)]
    std::fs::hard_link(
        source.path().join("nested/input.txt"),
        source.path().join("nested/input.hard"),
    )?;
    std::fs::write(source.path().join("empty.txt"), [])?;
    std::fs::write(source.path().join("mode.txt"), b"mode")?;
    std::fs::write(source.path().join("café.txt"), b"unicode")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{symlink, PermissionsExt};

        std::fs::set_permissions(
            source.path().join("nested"),
            std::fs::Permissions::from_mode(0o750),
        )?;
        std::fs::set_permissions(
            source.path().join("nested/input.txt"),
            std::fs::Permissions::from_mode(0o640),
        )?;
        std::fs::set_permissions(
            source.path().join("mode.txt"),
            std::fs::Permissions::from_mode(0o600),
        )?;
        xattr::set(
            source.path().join("nested"),
            "user.bollard.directory",
            b"directory metadata",
        )?;
        xattr::set(
            source.path().join("nested/input.txt"),
            "user.bollard.file",
            b"file metadata",
        )?;
        symlink("nested/input.txt", source.path().join("link"))?;
    }
    Ok(source)
}

#[cfg(unix)]
fn xattrs_for_path(path: &Path) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    let mut values = BTreeMap::new();
    for name in xattr::list(path)? {
        if let Some(name_str) = name.to_str() {
            if let Some(value) = xattr::get(path, &name)? {
                values.insert(name_str.to_owned(), value);
            }
        }
    }
    Ok(values)
}

fn relative_entries(root: &Path) -> Result<BTreeSet<PathBuf>, Error> {
    fn visit(root: &Path, path: &Path, entries: &mut BTreeSet<PathBuf>) -> Result<(), Error> {
        for item in std::fs::read_dir(path)? {
            let item = item?;
            let path = item.path();
            let relative = path.strip_prefix(root).map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("failed to relativize fixture path: {error}")),
            })?;
            entries.insert(relative.to_path_buf());
            if item.file_type()?.is_dir() {
                visit(root, &path, entries)?;
            }
        }
        Ok(())
    }

    let mut entries = BTreeSet::new();
    visit(root, root, &mut entries)?;
    Ok(entries)
}

fn assert_tree_equal(expected: &Path, actual: &Path) -> Result<(), Error> {
    let expected_entries = relative_entries(expected)?;
    let actual_entries = relative_entries(actual)?;
    assert_eq!(expected_entries, actual_entries);

    for relative in &expected_entries {
        let expected_path = expected.join(relative);
        let actual_path = actual.join(relative);
        let expected_type = std::fs::symlink_metadata(&expected_path)?.file_type();
        let actual_type = std::fs::symlink_metadata(&actual_path)?.file_type();
        assert_eq!(expected_type.is_dir(), actual_type.is_dir(), "{relative:?}");
        assert_eq!(
            expected_type.is_file(),
            actual_type.is_file(),
            "{relative:?}"
        );
        assert_eq!(
            expected_type.is_symlink(),
            actual_type.is_symlink(),
            "{relative:?}"
        );

        if expected_type.is_symlink() {
            assert_eq!(
                std::fs::read_link(&expected_path)?,
                std::fs::read_link(&actual_path)?,
                "{relative:?}"
            );
        } else if expected_type.is_file() {
            assert_eq!(
                std::fs::read(&expected_path)?,
                std::fs::read(&actual_path)?,
                "{relative:?}"
            );
        }

        #[cfg(unix)]
        if !expected_type.is_symlink() {
            assert_eq!(
                xattrs_for_path(&expected_path)?,
                xattrs_for_path(&actual_path)?,
                "{relative:?} xattrs"
            );
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            assert_eq!(
                std::fs::symlink_metadata(expected_path)?
                    .permissions()
                    .mode()
                    & 0o777,
                std::fs::symlink_metadata(actual_path)?.permissions().mode() & 0o777,
                "{relative:?} mode"
            );
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        fn hardlink_groups(
            root: &Path,
            entries: &BTreeSet<PathBuf>,
        ) -> Result<BTreeSet<BTreeSet<PathBuf>>, Error> {
            let mut groups = BTreeMap::<(u64, u64), BTreeSet<PathBuf>>::new();
            for relative in entries {
                let path = root.join(relative);
                let metadata = std::fs::symlink_metadata(&path)?;
                if metadata.file_type().is_file() && metadata.nlink() > 1 {
                    groups
                        .entry((metadata.dev(), metadata.ino()))
                        .or_default()
                        .insert(relative.clone());
                }
            }
            Ok(groups
                .into_values()
                .filter(|paths| paths.len() > 1)
                .collect::<BTreeSet<_>>())
        }

        assert_eq!(
            hardlink_groups(expected, &expected_entries)?,
            hardlink_groups(actual, &actual_entries)?,
            "hardlink topology"
        );
    }
    Ok(())
}

fn unique_builder_name() -> String {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_nanos();
    format!("bollard_llb_gate_f_{suffix}")
}

async fn capture_builder_identity(docker: &Docker, driver_name: &str) -> Result<(), Error> {
    let container = docker.inspect_container(driver_name, None).await?;
    let image_ref = container
        .config
        .as_ref()
        .and_then(|config| config.image.clone())
        .ok_or_else(|| Error::IOError {
            err: std::io::Error::other("BuildKit container has no configured image"),
        })?;
    let container_id = container.id.clone().ok_or_else(|| Error::IOError {
        err: std::io::Error::other("BuildKit container has no resolved image ID"),
    })?;
    let image_id = container.image.clone().ok_or_else(|| Error::IOError {
        err: std::io::Error::other("BuildKit container has no image ID"),
    })?;
    let image = docker.inspect_image(&image_ref).await?;
    let repo_digests = image.repo_digests.unwrap_or_default();

    let exec = docker
        .create_exec(
            driver_name,
            CreateExecOptions {
                attach_stdout: Some(true),
                cmd: Some(vec![String::from("buildkitd"), String::from("--version")]),
                ..Default::default()
            },
        )
        .await?;
    let results = docker
        .start_exec(&exec.id, None::<StartExecOptions>)
        .await?;
    let version_output = match results {
        StartExecResults::Attached { output, .. } => {
            let output: Vec<LogOutput> = output.try_collect().await?;
            output
                .into_iter()
                .filter_map(|entry| match entry {
                    LogOutput::StdOut { message } => Some(message),
                    LogOutput::StdErr { .. } => None,
                    _ => None,
                })
                .flatten()
                .collect::<Vec<_>>()
        }
        StartExecResults::Detached => Vec::new(),
    };
    let version_output = String::from_utf8_lossy(&version_output).trim().to_string();
    let engine_version = docker.version().await?;
    let daemon_info = docker.info().await?;

    assert!(
        !version_output.is_empty(),
        "buildkitd --version returned no output"
    );
    assert!(
        !repo_digests.is_empty(),
        "BuildKit image has no repository digest"
    );
    println!(
        "phase-f identity: image_ref={image_ref} image_id={image_id} container_id={container_id} image_digests={repo_digests:?} buildkitd_version={version_output:?} docker_version={:?} docker_api_version={:?} docker_daemon_id={:?}",
        engine_version.version,
        engine_version.api_version,
        daemon_info.id
    );
    Ok(())
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

async fn local_source_application_mvp_test(docker: Docker) -> Result<(), Error> {
    let name = unique_builder_name();
    let volume_name = format!("{name}_state");
    let source = create_application_fixture()?;
    let direct_output = tempfile::tempdir()?;
    let app_output = tempfile::tempdir()?;
    let exec_output = tempfile::tempdir()?;
    let (image_ref, image_registry) = alpine_image_reference();

    let result = async {
        let mut builder = DockerContainerBuilder::new(&docker);
        builder.name(&name);
        let driver = builder.bootstrap().await.map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("BuildKit bootstrap failed: {error}")),
        })?;
        capture_builder_identity(&docker, driver.name()).await?;

        let request = DefinitionSolveRequest::new(
            local_source_copy_definition("context", "/"),
            DefinitionExporter::Local(direct_output.path().to_path_buf()),
        )
        .with_options(local_source_options("context", source.path(), None)?);
        SolveDefinition::solve_definition(&driver, request)
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("local source export failed: {error}")),
            })?;
        assert_tree_equal(source.path(), direct_output.path())?;

        let request = DefinitionSolveRequest::new(
            local_source_copy_definition("context", "/app"),
            DefinitionExporter::Local(app_output.path().to_path_buf()),
        )
        .with_options(local_source_options("context", source.path(), None)?);
        SolveDefinition::solve_definition(&driver, request)
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("local source copy failed: {error}")),
            })?;
        assert_tree_equal(source.path(), &app_output.path().join("app"))?;

        let request = DefinitionSolveRequest::new(
            local_source_exec_definition(&format!("docker-image://{image_ref}"), "context"),
            DefinitionExporter::Local(exec_output.path().to_path_buf()),
        )
        .with_options(local_source_options(
            "context",
            source.path(),
            image_registry.as_deref(),
        )?);
        SolveDefinition::solve_definition(&driver, request)
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("local source exec failed: {error}")),
            })?;
        assert_eq!(
            std::fs::read(exec_output.path().join("result.txt"))?,
            b"local source"
        );
        assert!(
            !std::fs::read_to_string(exec_output.path().join("cache-proof"))?
                .trim()
                .is_empty()
        );

        let container = docker.inspect_container(driver.name(), None).await?;
        assert_eq!(container.name.as_deref(), Some(format!("/{name}").as_str()));
        assert_eq!(docker.inspect_volume(&volume_name).await?.name, volume_name);
        Ok::<(), Error>(())
    }
    .await;

    let _ = driver_cleanup(&docker, &name, &volume_name).await;
    result
}

async fn local_source_filter_and_metadata_test(docker: Docker) -> Result<(), Error> {
    let name = unique_builder_name();
    let volume_name = format!("{name}_state");
    let source = create_application_fixture()?;
    let output = tempfile::tempdir()?;
    let encoded_output = tempfile::tempdir()?;

    let result = async {
        let mut builder = DockerContainerBuilder::new(&docker);
        builder.name(&name);
        let driver = builder.bootstrap().await.map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("BuildKit bootstrap failed: {error}")),
        })?;
        let request = DefinitionSolveRequest::new(
            local_source_copy_definition_with_patterns(
                "context",
                "/",
                &["**/*.txt"],
                &["nested/input.txt"],
            ),
            DefinitionExporter::Local(output.path().to_path_buf()),
        )
        .with_options(local_source_options("context", source.path(), None)?);
        SolveDefinition::solve_definition(&driver, request)
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("filtered local source solve failed: {error}")),
            })?;

        let entries = relative_entries(output.path())?;
        let expected = BTreeSet::from([
            PathBuf::from("café.txt"),
            PathBuf::from("empty.txt"),
            PathBuf::from("mode.txt"),
        ]);
        assert_eq!(entries, expected);
        assert_eq!(std::fs::read(output.path().join("café.txt"))?, b"unicode");

        let request = DefinitionSolveRequest::new(
            local_source_copy_definition_with_patterns("context", "/", &["café.txt"], &[]),
            DefinitionExporter::Local(encoded_output.path().to_path_buf()),
        )
        .with_options(local_source_options("context", source.path(), None)?);
        SolveDefinition::solve_definition(&driver, request)
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("encoded local source solve failed: {error}")),
            })?;
        assert_eq!(
            relative_entries(encoded_output.path())?,
            BTreeSet::from([PathBuf::from("café.txt")])
        );
        Ok::<(), Error>(())
    }
    .await;

    let _ = driver_cleanup(&docker, &name, &volume_name).await;
    result
}

async fn local_source_cache_repeatability_test(docker: Docker) -> Result<(), Error> {
    let name = unique_builder_name();
    let volume_name = format!("{name}_state");
    let source = create_application_fixture()?;
    let first_output = tempfile::tempdir()?;
    let second_output = tempfile::tempdir()?;
    let (image_ref, image_registry) = alpine_image_reference();

    let result = async {
        let mut builder = DockerContainerBuilder::new(&docker);
        builder.name(&name);
        let driver = builder.bootstrap().await.map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("BuildKit bootstrap failed: {error}")),
        })?;
        let first_id = docker
            .inspect_container(driver.name(), None)
            .await?
            .id
            .ok_or_else(|| Error::IOError {
                err: std::io::Error::other("first BuildKit container has no ID"),
            })?;
        let definition =
            local_source_exec_definition(&format!("docker-image://{image_ref}"), "context");

        for (output, label) in [(&first_output, "first"), (&second_output, "second")] {
            let request = DefinitionSolveRequest::new(
                definition.clone(),
                DefinitionExporter::Local(output.path().to_path_buf()),
            )
            .with_options(local_source_options(
                "context",
                source.path(),
                image_registry.as_deref(),
            )?);
            SolveDefinition::solve_definition(&driver, request)
                .await
                .map_err(|error| Error::IOError {
                    err: std::io::Error::other(format!("{label} cached solve failed: {error}")),
                })?;
            assert_eq!(
                std::fs::read(output.path().join("result.txt"))?,
                b"local source"
            );
            if label == "first" {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }

        assert_eq!(
            std::fs::read_to_string(first_output.path().join("cache-proof"))?,
            std::fs::read_to_string(second_output.path().join("cache-proof"))?
        );
        let second_id = docker
            .inspect_container(driver.name(), None)
            .await?
            .id
            .ok_or_else(|| Error::IOError {
                err: std::io::Error::other("second BuildKit container has no ID"),
            })?;
        assert_eq!(first_id, second_id);
        assert_eq!(docker.inspect_volume(&volume_name).await?.name, volume_name);
        Ok::<(), Error>(())
    }
    .await;

    let _ = driver_cleanup(&docker, &name, &volume_name).await;
    result
}

async fn local_source_unknown_name_test(docker: Docker) -> Result<(), Error> {
    let name = unique_builder_name();
    let volume_name = format!("{name}_state");
    let context = create_application_fixture()?;
    let other = create_application_fixture()?;
    std::fs::write(context.path().join("context-only.txt"), b"context")?;
    std::fs::write(other.path().join("other-only.txt"), b"other")?;
    let failed_output = tempfile::tempdir()?;
    let successful_output = tempfile::tempdir()?;

    let result = async {
        let mut builder = DockerContainerBuilder::new(&docker);
        builder.name(&name);
        let driver = builder.bootstrap().await.map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("BuildKit bootstrap failed: {error}")),
        })?;
        let options = DefinitionSolveOptionsBuilder::new()
            .local_mount("context", context.path())
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("context mount setup failed: {error}")),
            })?
            .local_mount("other", other.path())
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("other mount setup failed: {error}")),
            })?
            .build();
        let request = DefinitionSolveRequest::new(
            local_source_copy_definition("missing", "/"),
            DefinitionExporter::Local(failed_output.path().to_path_buf()),
        )
        .with_options(options.clone());
        let error = SolveDefinition::solve_definition(&driver, request)
            .await
            .expect_err("unknown local source should fail");
        match error {
            bollard::grpc::error::GrpcError::TonicStatus { err } => {
                assert_eq!(err.code(), tonic::Code::NotFound)
            }
            error => panic!("unknown local source returned an unexpected error: {error:?}"),
        }
        assert!(relative_entries(failed_output.path())?.is_empty());

        let request = DefinitionSolveRequest::new(
            local_source_copy_definition("context", "/"),
            DefinitionExporter::Local(successful_output.path().to_path_buf()),
        )
        .with_options(options);
        SolveDefinition::solve_definition(&driver, request)
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("registered local source failed: {error}")),
            })?;
        assert_tree_equal(context.path(), successful_output.path())?;
        assert!(!successful_output.path().join("other-only.txt").exists());
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
fn integration_test_local_source_filesync() {
    connect_to_docker_and_run!(local_source_application_mvp_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_local_source_cache_repeatability() {
    connect_to_docker_and_run!(local_source_cache_repeatability_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_local_source_filter_and_metadata() {
    connect_to_docker_and_run!(local_source_filter_and_metadata_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_unknown_local_source_isolated() {
    connect_to_docker_and_run!(local_source_unknown_name_test);
}

#[test]
fn direct_definition_request_accepts_known_valid_protobuf() {
    let request = DefinitionSolveRequest::new(
        minimal_mkfile_definition(),
        DefinitionExporter::Local(std::path::PathBuf::from("/tmp/output")),
    );
    assert_eq!(request.definition.def.len(), 2);
}
