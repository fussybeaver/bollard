#![cfg(feature = "buildkit")]

use bollard::errors::Error;
use bollard::grpc::build::SecretSource;
use bollard::grpc::driver::docker_container::DockerContainer;
use bollard::grpc::driver::{
    DefinitionExporter, DefinitionSolveOptionsBuilder, DefinitionSolveRequest, SolveDefinition,
};
use bollard::Docker;
use bollard_buildkit_proto::pb;
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

const MKFILE_GOLDEN: &[u8] = include_bytes!("../llb/testdata/golden/mkfile.llb.pb");
const SYMLINK_GOLDEN: &[u8] = include_bytes!("../llb/testdata/golden/symlink.llb.pb");
const IMAGE_GOLDEN: &[u8] = include_bytes!("../llb/testdata/golden/image_run.llb.pb");
const DIFFERENTIAL_MERGE_GOLDEN: &[u8] =
    include_bytes!("../llb/testdata/golden/differential_merge_alpine.llb.pb");
const DIFFERENTIAL_FILE_SECRET_GOLDEN: &[u8] =
    include_bytes!("../llb/testdata/golden/differential_file_secret.llb.pb");
const DIFFERENTIAL_ENV_SECRET_GOLDEN: &[u8] =
    include_bytes!("../llb/testdata/golden/differential_env_secret.llb.pb");
const DIFFERENTIAL_FILE_OPS_GOLDEN: &[u8] =
    include_bytes!("../llb/testdata/golden/differential_file_operations_allow_not_found.llb.pb");

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

    for relative in expected_entries {
        let expected_path = expected.join(&relative);
        let actual_path = actual.join(&relative);
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
    Ok(())
}

#[derive(Debug, PartialEq)]
enum ExportEntry {
    File { mode: u32, contents: Vec<u8> },
    Dir { mode: u32 },
    Symlink { target: String },
}

#[cfg(unix)]
fn export_mode(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;

    path.symlink_metadata()
        .expect("export entry metadata should be readable")
        .mode()
        & 0o777
}

#[cfg(not(unix))]
fn export_mode(_path: &Path) -> u32 {
    0
}

fn read_export_tree(root: &Path) -> Result<BTreeMap<PathBuf, ExportEntry>, Error> {
    fn visit(
        root: &Path,
        current: &Path,
        tree: &mut BTreeMap<PathBuf, ExportEntry>,
    ) -> Result<(), Error> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(root).map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("failed to relativize export path: {error}")),
            })?;
            let metadata = path.symlink_metadata()?;
            let value = if metadata.file_type().is_symlink() {
                ExportEntry::Symlink {
                    target: std::fs::read_link(&path)?.to_string_lossy().into_owned(),
                }
            } else if metadata.file_type().is_dir() {
                let value = ExportEntry::Dir {
                    mode: export_mode(&path),
                };
                tree.insert(relative.to_path_buf(), value);
                visit(root, &path, tree)?;
                continue;
            } else {
                ExportEntry::File {
                    mode: export_mode(&path),
                    contents: std::fs::read(&path)?,
                }
            };
            tree.insert(relative.to_path_buf(), value);
        }
        Ok(())
    }

    let mut tree = BTreeMap::new();
    visit(root, root, &mut tree)?;
    Ok(tree)
}

fn unique_builder_name() -> String {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_nanos();
    format!("bollard_llb_gate_f_{suffix}")
}

fn llb_error(error: bollard_llb::LlbError) -> Error {
    Error::IOError {
        err: std::io::Error::other(format!("LLB definition construction failed: {error}")),
    }
}

fn go_definition(bytes: &[u8]) -> Result<pb::Definition, Error> {
    pb::Definition::decode(bytes).map_err(|error| Error::IOError {
        err: std::io::Error::other(format!("failed to decode Go definition: {error}")),
    })
}

fn registry_image(name: &str) -> String {
    format!("{}{name}", crate::common::registry_http_addr())
}

fn differential_mkfile_definition() -> Result<pb::Definition, Error> {
    use bollard_llb::{mkfile, scratch, FileOpts, MarshalOpts};

    Ok(scratch()
        .map_err(llb_error)?
        .file(mkfile("/hello", 0o644, b"world"), FileOpts::new())
        .map_err(llb_error)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_error)?
        .to_pb())
}

fn differential_symlink_definition() -> Result<pb::Definition, Error> {
    use bollard_llb::{scratch, symlink, FileOpts, MarshalOpts};

    Ok(scratch()
        .map_err(llb_error)?
        .file(symlink("/target", "/link"), FileOpts::new())
        .map_err(llb_error)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_error)?
        .to_pb())
}

fn differential_image_definition() -> Result<pb::Definition, Error> {
    use bollard_llb::{image, shlex, MarshalOpts};

    Ok(image(registry_image("alpine:latest"))
        .map_err(llb_error)?
        .run(shlex("echo hello").map_err(llb_error)?)
        .root()
        .map_err(llb_error)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_error)?
        .to_pb())
}

fn differential_merge_definition() -> Result<pb::Definition, Error> {
    use bollard_llb::{image, merge, shlex, MarshalOpts, MergeOpts};

    let first = image(registry_image("alpine:latest")).map_err(llb_error)?;
    let second = image(registry_image("alpine:latest")).map_err(llb_error)?;
    Ok(merge(vec![first, second], MergeOpts::new())
        .map_err(llb_error)?
        .run(shlex("sh -c 'echo differential > /differential'").map_err(llb_error)?)
        .root()
        .map_err(llb_error)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_error)?
        .to_pb())
}

fn differential_file_secret_definition() -> Result<pb::Definition, Error> {
    use bollard_llb::{image, AddSecret, MarshalOpts, RunOpts};

    Ok(image(registry_image("alpine:latest"))
        .map_err(llb_error)?
        .run(
            RunOpts::default()
                .with_arg("sh")
                .with_arg("-c")
                .with_arg("sha256sum /run/secrets/token > /derived"),
        )
        .add_secret(
            "token",
            AddSecret {
                target: Some(String::from("/run/secrets/token")),
                ..Default::default()
            },
        )
        .root()
        .map_err(llb_error)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_error)?
        .to_pb())
}

fn differential_env_secret_definition() -> Result<pb::Definition, Error> {
    use bollard_llb::{image, AddSecret, MarshalOpts, RunOpts};

    Ok(image(registry_image("alpine:latest"))
        .map_err(llb_error)?
        .run(
            RunOpts::default()
                .with_arg("sh")
                .with_arg("-c")
                .with_arg("printf '%s' \"$MY_SECRET\" | sha256sum > /derived"),
        )
        .add_secret(
            "mysecret",
            AddSecret {
                as_env: true,
                env_name: Some(String::from("MY_SECRET")),
                ..Default::default()
            },
        )
        .root()
        .map_err(llb_error)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_error)?
        .to_pb())
}

fn differential_file_operations_definition() -> Result<pb::Definition, Error> {
    use bollard_llb::{mkdir, mkfile, rm, symlink, FileOpts, MarshalOpts};

    let state = bollard_llb::scratch()
        .map_err(llb_error)?
        .file(mkdir("/app", 0o755).with_parents(true), FileOpts::new())
        .map_err(llb_error)?
        .file(
            mkfile("/app/config.toml", 0o644, b"[server]\nhost = \"0.0.0.0\"\n"),
            FileOpts::new(),
        )
        .map_err(llb_error)?
        .file(
            symlink("/app/config.toml", "/app/current-config"),
            FileOpts::new(),
        )
        .map_err(llb_error)?
        .file(
            rm("/app/current-config").with_allow_not_found(true),
            FileOpts::new(),
        )
        .map_err(llb_error)?;

    Ok(state
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_error)?
        .to_pb())
}

async fn solve_definition_with_driver(
    driver: &DockerContainer,
    definition: pb::Definition,
    destination: &Path,
    image_registry: Option<&str>,
    secrets: Vec<(String, SecretSource)>,
) -> Result<(), Error> {
    let mut options = DefinitionSolveOptionsBuilder::new();
    if let Some(host) = image_registry {
        options = options.credential(host, integration_test_registry_credentials());
    }
    for (id, source) in secrets {
        options = options.secret(id, source);
    }

    let request = DefinitionSolveRequest::new(
        definition,
        DefinitionExporter::Local(destination.to_path_buf()),
    )
    .with_options(options.build());
    SolveDefinition::solve_definition(driver, request)
        .await
        .map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("direct definition solve failed: {error}")),
        })
}

async fn differential_exported_tree_test(docker: Docker) -> Result<(), Error> {
    type DefinitionBuilder = fn() -> Result<pb::Definition, Error>;

    let cases: [(&str, &[u8], DefinitionBuilder); 7] = [
        ("mkfile", MKFILE_GOLDEN, differential_mkfile_definition),
        ("symlink", SYMLINK_GOLDEN, differential_symlink_definition),
        ("image", IMAGE_GOLDEN, differential_image_definition),
        (
            "merge_alpine",
            DIFFERENTIAL_MERGE_GOLDEN,
            differential_merge_definition,
        ),
        (
            "file_secret",
            DIFFERENTIAL_FILE_SECRET_GOLDEN,
            differential_file_secret_definition,
        ),
        (
            "env_secret",
            DIFFERENTIAL_ENV_SECRET_GOLDEN,
            differential_env_secret_definition,
        ),
        (
            "file_operations_allow_not_found",
            DIFFERENTIAL_FILE_OPS_GOLDEN,
            differential_file_operations_definition,
        ),
    ];

    let name = unique_builder_name();
    let volume_name = format!("{name}_state");
    let (image_ref, image_registry) = alpine_image_reference();
    let secret_dir = tempfile::tempdir()?;
    let token_path = secret_dir.path().join("token");
    std::fs::write(&token_path, "phase5-differential-secret")?;
    std::env::set_var(
        "PHASE5_DIFFERENTIAL_ENV_SECRET",
        "phase5-differential-env-secret",
    );

    let result = async {
        let mut builder = common::buildkit_test::builder(&docker);
        builder.name(&name);
        let driver = builder.bootstrap().await.map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("BuildKit bootstrap failed: {error}")),
        })?;
        println!(
            "{}",
            common::buildkit_test::record_version(&docker, &driver).await?
        );

        for (fixture, golden, rust_builder) in cases {
            let go_destination = tempfile::tempdir()?;
            let rust_destination = tempfile::tempdir()?;
            let go_secrets = match fixture {
                "file_secret" => vec![(
                    String::from("token"),
                    SecretSource::File(token_path.clone()),
                )],
                "env_secret" => vec![(
                    String::from("mysecret"),
                    SecretSource::Env(String::from("PHASE5_DIFFERENTIAL_ENV_SECRET")),
                )],
                _ => Vec::new(),
            };
            let rust_secrets = match fixture {
                "file_secret" => vec![(
                    String::from("token"),
                    SecretSource::File(token_path.clone()),
                )],
                "env_secret" => vec![(
                    String::from("mysecret"),
                    SecretSource::Env(String::from("PHASE5_DIFFERENTIAL_ENV_SECRET")),
                )],
                _ => Vec::new(),
            };

            solve_definition_with_driver(
                &driver,
                go_definition(golden)?,
                go_destination.path(),
                image_registry.as_deref(),
                go_secrets,
            )
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("{fixture}: Go solve failed: {error}")),
            })?;
            solve_definition_with_driver(
                &driver,
                rust_builder()?,
                rust_destination.path(),
                image_registry.as_deref(),
                rust_secrets,
            )
            .await
            .map_err(|error| Error::IOError {
                err: std::io::Error::other(format!("{fixture}: Rust solve failed: {error}")),
            })?;

            assert_eq!(
                read_export_tree(go_destination.path())?,
                read_export_tree(rust_destination.path())?,
                "Go/Rust exported filesystem mismatch for {fixture} using {image_ref}"
            );
        }
        Ok::<(), Error>(())
    }
    .await;

    let _ = driver_cleanup(&docker, &name, &volume_name).await;
    result
}

async fn direct_definition_solve_test(docker: Docker) -> Result<(), Error> {
    let name = unique_builder_name();
    let volume_name = format!("{name}_state");
    let first_output = tempfile::tempdir()?;
    let second_output = tempfile::tempdir()?;

    let result = async {
        let mut builder = common::buildkit_test::builder(&docker);
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
        let mut builder = common::buildkit_test::builder(&docker);
        builder.name(&name);
        let driver = builder.bootstrap().await.map_err(|error| Error::IOError {
            err: std::io::Error::other(format!("BuildKit bootstrap failed: {error}")),
        })?;
        println!(
            "{}",
            common::buildkit_test::record_version(&docker, &driver).await?
        );

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
        let mut builder = common::buildkit_test::builder(&docker);
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
        let mut builder = common::buildkit_test::builder(&docker);
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
        let mut builder = common::buildkit_test::builder(&docker);
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
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_go_rust_differential() {
    connect_to_docker_and_run!(differential_exported_tree_test);
}

#[test]
fn direct_definition_request_accepts_known_valid_protobuf() {
    let request = DefinitionSolveRequest::new(
        minimal_mkfile_definition(),
        DefinitionExporter::Local(std::path::PathBuf::from("/tmp/output")),
    );
    assert_eq!(request.definition.def.len(), 2);
}
