#![cfg(feature = "buildkit")]

use bollard::auth::DockerCredentials;
use bollard::errors::Error;
use bollard::grpc::build::SecretSource;
use bollard::grpc::driver::{
    DefinitionExporter, DefinitionSolveOptions, DefinitionSolveRequest, SolveDefinition,
};
use bollard::Docker;
use bollard_buildkit_proto::pb;
use prost::Message;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use tokio::runtime::Runtime;

#[macro_use]
pub mod common;
use crate::common::*;

const MKFILE_GOLDEN: &[u8] = include_bytes!("../llb/testdata/golden/mkfile.llb.pb");
const SYMLINK_GOLDEN: &[u8] = include_bytes!("../llb/testdata/golden/symlink.llb.pb");
const IMAGE_GOLDEN: &[u8] = include_bytes!("../llb/testdata/golden/image_run.llb.pb");
const FILE_OPS_GOLDEN: &[u8] =
    include_bytes!("../llb/testdata/golden/file_operations_chain.llb.pb");

#[derive(Debug, PartialEq)]
enum ExportEntry {
    File { mode: u32, contents: Vec<u8> },
    Dir { mode: u32 },
    Symlink { target: String },
}

fn llb_err(e: bollard_llb::LlbError) -> Error {
    Error::IOError {
        err: std::io::Error::other(format!("llb error: {e}")),
    }
}

fn go_definition(bytes: &[u8]) -> Result<pb::Definition, Error> {
    pb::Definition::decode(bytes).map_err(|e| Error::IOError {
        err: std::io::Error::other(format!("failed to decode golden definition: {e}")),
    })
}

fn registry_credentials() -> HashMap<String, DockerCredentials> {
    let mut map = HashMap::new();
    let host = crate::common::registry_http_addr()
        .trim_end_matches('/')
        .to_string();
    if !host.is_empty() {
        map.insert(host, crate::common::integration_test_registry_credentials());
    }
    map
}

fn registry_image(name: &str) -> String {
    format!("{}{name}", crate::common::registry_http_addr())
}

async fn solve_to_dir(
    docker: &Docker,
    definition: pb::Definition,
    dest: &Path,
    secrets: HashMap<String, SecretSource>,
    container_name: Option<&str>,
) -> Result<(), Error> {
    let mut builder = crate::common::buildkit_test::builder(docker);
    if let Some(name) = container_name {
        builder.name(name);
    }
    let driver = builder.bootstrap().await.map_err(|e| Error::IOError {
        err: std::io::Error::other(format!("buildkit bootstrap failed: {e}")),
    })?;

    let version_record = crate::common::buildkit_test::record_version(docker, &driver).await;
    if let Ok(record) = version_record.as_ref() {
        println!("{}", record);
    }

    let options = DefinitionSolveOptions {
        credentials: Some(registry_credentials()),
        secrets,
        ..Default::default()
    };

    let request =
        DefinitionSolveRequest::new(definition, DefinitionExporter::Local(dest.to_path_buf()))
            .with_options(options);

    let solve_result = SolveDefinition::solve_definition(driver, request).await;
    version_record?;
    solve_result.map_err(|e| Error::IOError {
        err: std::io::Error::other(format!("solve_definition failed: {e}")),
    })
}

fn read_export_entry(root: &Path, path: &str) -> Result<ExportEntry, Error> {
    let full = root.join(path);
    let meta = full.symlink_metadata()?;
    let mode = meta.mode() & 0o777;

    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(&full)?;
        Ok(ExportEntry::Symlink {
            target: target.to_string_lossy().to_string(),
        })
    } else if meta.file_type().is_dir() {
        Ok(ExportEntry::Dir { mode })
    } else {
        let contents = std::fs::read(&full)?;
        Ok(ExportEntry::File { mode, contents })
    }
}

fn read_export_tree(root: &Path) -> Result<BTreeMap<PathBuf, ExportEntry>, Error> {
    let mut tree = BTreeMap::new();
    read_export_tree_recursive(root, root, &mut tree)?;
    Ok(tree)
}

fn read_export_tree_recursive(
    root: &Path,
    current: &Path,
    tree: &mut BTreeMap<PathBuf, ExportEntry>,
) -> Result<(), Error> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let meta = entry.path().symlink_metadata()?;
        let mode = meta.mode() & 0o777;

        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(&path)?;
            tree.insert(
                rel,
                ExportEntry::Symlink {
                    target: target.to_string_lossy().to_string(),
                },
            );
        } else if meta.file_type().is_dir() {
            tree.insert(rel.clone(), ExportEntry::Dir { mode });
            read_export_tree_recursive(root, &path, tree)?;
        } else {
            let contents = std::fs::read(&path)?;
            tree.insert(rel, ExportEntry::File { mode, contents });
        }
    }
    Ok(())
}

fn assert_exported_file(dest: &Path, path: &str, expected: &[u8], expected_mode: u32) {
    let entry = read_export_entry(dest, path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    match entry {
        ExportEntry::File { mode, contents } => {
            assert_eq!(contents, expected, "contents mismatch for {path}");
            assert_eq!(mode, expected_mode, "mode mismatch for {path}");
        }
        other => panic!("expected {path} to be a file, got {other:?}"),
    }
}

fn assert_exported_symlink(dest: &Path, path: &str, expected_target: &str) {
    let entry = read_export_entry(dest, path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    match entry {
        ExportEntry::Symlink { target } => {
            assert_eq!(
                target, expected_target,
                "symlink target mismatch for {path}"
            );
        }
        other => panic!("expected {path} to be a symlink, got {other:?}"),
    }
}

fn assert_exported_absent(dest: &Path, path: &str) {
    let full = dest.join(path);
    assert!(!full.exists(), "expected {path} to be absent from export");
}

fn scratch_mkfile_def() -> Result<pb::Definition, Error> {
    use bollard_llb::{mkfile, scratch, FileOpts, MarshalOpts};
    Ok(scratch()
        .map_err(llb_err)?
        .file(mkfile("/hello", 0o644, b"world"), FileOpts::new())
        .map_err(llb_err)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_err)?
        .to_pb())
}

fn image_exec_def() -> Result<pb::Definition, Error> {
    use bollard_llb::{image, shlex, MarshalOpts, State};
    let state: State = image(registry_image("alpine")).map_err(llb_err)?;
    Ok(state
        .run(shlex("sh -c 'echo hello > /phase4-image'"))
        .root()
        .map_err(llb_err)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_err)?
        .to_pb())
}

fn file_operations_def() -> Result<pb::Definition, Error> {
    use bollard_llb::{copy, mkdir, mkfile, rm, scratch, symlink, FileOpts, MarshalOpts};
    let s = scratch().map_err(llb_err)?;
    let s = s
        .file(mkdir("/app", 0o755).with_parents(true), FileOpts::new())
        .map_err(llb_err)?;
    let s = s
        .file(
            mkfile("/app/config.toml", 0o644, b"[server]\nhost = \"0.0.0.0\"\n"),
            FileOpts::new(),
        )
        .map_err(llb_err)?;
    let s = s
        .file(
            symlink("/app/config.toml", "/app/current-config"),
            FileOpts::new(),
        )
        .map_err(llb_err)?;
    let src_for_copy = s.clone();
    let s = s
        .file(
            copy(src_for_copy, "/app/config.toml", "/app/config.toml.bak")
                .with_create_dest_path(true),
            FileOpts::new(),
        )
        .map_err(llb_err)?;
    let s = s
        .file(rm("/app/current-config"), FileOpts::new())
        .map_err(llb_err)?;
    Ok(s.marshal(MarshalOpts::linux_amd64())
        .map_err(llb_err)?
        .to_pb())
}

fn file_secret_def() -> Result<pb::Definition, Error> {
    use bollard_llb::{image, AddSecret, MarshalOpts, RunOpts, State};
    let state: State = image(registry_image("alpine")).map_err(llb_err)?;
    Ok(state
        .run(
            RunOpts::default()
                .with_arg("sh")
                .with_arg("-c")
                .with_arg("sha256sum /run/secrets/token > /derived"),
        )
        .add_secret(
            "token",
            AddSecret {
                id: String::new(),
                as_env: false,
                env_name: None,
                target: Some("/run/secrets/token".into()),
                optional: false,
            },
        )
        .root()
        .map_err(llb_err)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_err)?
        .to_pb())
}

fn env_secret_def() -> Result<pb::Definition, Error> {
    use bollard_llb::{image, AddSecret, MarshalOpts, RunOpts, State};
    let state: State = image(registry_image("alpine")).map_err(llb_err)?;
    Ok(state
        .run(
            RunOpts::default()
                .with_arg("sh")
                .with_arg("-c")
                .with_arg("printf '%s' \"$MY_SECRET\" | sha256sum > /derived"),
        )
        .add_secret(
            "mysecret",
            AddSecret {
                id: String::new(),
                as_env: true,
                env_name: Some("MY_SECRET".into()),
                target: None,
                optional: false,
            },
        )
        .root()
        .map_err(llb_err)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_err)?
        .to_pb())
}

fn merge_cache_def() -> Result<pb::Definition, Error> {
    use bollard_llb::{image, merge, shlex, CacheSharingMode, MarshalOpts, MergeOpts, State};
    let a: State = image(registry_image("alpine")).map_err(llb_err)?;
    let a = a.run(shlex("echo from-a > /a")).root().map_err(llb_err)?;
    let b: State = image(registry_image("alpine")).map_err(llb_err)?;
    let b = b.run(shlex("echo from-b > /b")).root().map_err(llb_err)?;
    let merged = merge(vec![a, b], MergeOpts::new()).map_err(llb_err)?;
    Ok(merged
        .run(shlex("cat /a /b > /result"))
        .add_mount_cache("/cache", "phase4-cache", CacheSharingMode::Shared)
        .root()
        .map_err(llb_err)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_err)?
        .to_pb())
}

fn cache_proof_def() -> Result<pb::Definition, Error> {
    use bollard_llb::{image, shlex, MarshalOpts, State};
    let state: State = image(registry_image("alpine")).map_err(llb_err)?;
    Ok(state
        .run(shlex("sh -c 'date +%s > /cache-proof'"))
        .root()
        .map_err(llb_err)?
        .marshal(MarshalOpts::linux_amd64())
        .map_err(llb_err)?
        .to_pb())
}

async fn llb_solve_mkfile_test(docker: Docker) -> Result<(), Error> {
    let dest = tempfile::tempdir()?;
    let def = go_definition(MKFILE_GOLDEN)?;
    solve_to_dir(&docker, def, dest.path(), HashMap::new(), None).await?;
    assert_exported_file(dest.path(), "hello", b"world", 0o644);
    Ok(())
}

async fn llb_solve_golden_symlink_test(docker: Docker) -> Result<(), Error> {
    let dest = tempfile::tempdir()?;
    let def = go_definition(SYMLINK_GOLDEN)?;
    solve_to_dir(&docker, def, dest.path(), HashMap::new(), None).await?;
    assert_exported_symlink(dest.path(), "link", "/target");
    Ok(())
}

async fn llb_solve_golden_image_test(docker: Docker) -> Result<(), Error> {
    let dest = tempfile::tempdir()?;
    let def = go_definition(IMAGE_GOLDEN)?;
    solve_to_dir(&docker, def, dest.path(), HashMap::new(), None).await?;
    assert!(dest.path().join("etc/alpine-release").is_file());
    Ok(())
}

async fn llb_solve_golden_file_operations_test(docker: Docker) -> Result<(), Error> {
    let dest = tempfile::tempdir()?;
    let def = go_definition(FILE_OPS_GOLDEN)?;
    solve_to_dir(&docker, def, dest.path(), HashMap::new(), None).await?;

    let tree = read_export_tree(dest.path())?;
    assert!(tree.contains_key(&PathBuf::from("app")));
    assert_eq!(
        tree.get(&PathBuf::from("app/config.toml")),
        Some(&ExportEntry::File {
            mode: 0o644,
            contents: b"[server]\nhost = \"0.0.0.0\"\n".to_vec(),
        })
    );
    assert_eq!(
        tree.get(&PathBuf::from("app/config.toml.bak")),
        Some(&ExportEntry::File {
            mode: 0o644,
            contents: b"[server]\nhost = \"0.0.0.0\"\n".to_vec(),
        })
    );
    assert_exported_absent(dest.path(), "app/current-config");
    Ok(())
}

async fn llb_solve_scratch_mkfile_test(docker: Docker) -> Result<(), Error> {
    let dest = tempfile::tempdir()?;
    let def = scratch_mkfile_def()?;
    let res = solve_to_dir(&docker, def, dest.path(), HashMap::new(), None).await;
    assert!(
        res.is_err(),
        "expected Rust scratch:// definition to fail until Phase 5"
    );
    Ok(())
}

async fn llb_solve_file_operations_test(docker: Docker) -> Result<(), Error> {
    let dest = tempfile::tempdir()?;
    let def = file_operations_def()?;
    let res = solve_to_dir(&docker, def, dest.path(), HashMap::new(), None).await;
    assert!(
        res.is_err(),
        "expected scratch-based file operations to fail until Phase 5"
    );
    Ok(())
}

async fn llb_solve_image_exec_test(docker: Docker) -> Result<(), Error> {
    let dest = tempfile::tempdir()?;
    let def = image_exec_def()?;
    solve_to_dir(&docker, def, dest.path(), HashMap::new(), None).await?;
    assert_exported_file(dest.path(), "phase4-image", b"hello\n", 0o644);
    Ok(())
}

async fn llb_solve_file_secret_test(docker: Docker) -> Result<(), Error> {
    let dest = tempfile::tempdir()?;
    let token = "phase4-secret-token";
    let token_path = dest.path().join("token");
    std::fs::write(&token_path, token)?;

    let mut secrets = HashMap::new();
    secrets.insert("token".to_string(), SecretSource::File(token_path));

    let out_dir = dest.path().join("out");
    std::fs::create_dir(&out_dir)?;
    let def = file_secret_def()?;
    let res = solve_to_dir(&docker, def, &out_dir, secrets, None).await;
    assert!(
        res.is_err(),
        "expected file-secret solve to fail until Phase 5 adds the secret mount"
    );
    Ok(())
}

async fn llb_solve_env_secret_test(docker: Docker) -> Result<(), Error> {
    let dest = tempfile::tempdir()?;
    let secret_value = "phase4-env-secret";
    std::env::set_var("PHASE4_SECRET_VALUE", secret_value);

    let mut secrets = HashMap::new();
    secrets.insert(
        "mysecret".to_string(),
        SecretSource::Env("PHASE4_SECRET_VALUE".to_string()),
    );

    let def = env_secret_def()?;
    solve_to_dir(&docker, def, dest.path(), secrets, None).await?;

    let expected = format!("{:x}  -\n", Sha256::digest(secret_value.as_bytes()));
    assert_exported_file(dest.path(), "derived", expected.as_bytes(), 0o644);
    Ok(())
}

async fn llb_solve_merge_cache_test(docker: Docker) -> Result<(), Error> {
    let dest = tempfile::tempdir()?;
    let def = merge_cache_def()?;
    solve_to_dir(&docker, def, dest.path(), HashMap::new(), None).await?;
    assert_exported_file(dest.path(), "result", b"from-a\nfrom-b\n", 0o644);
    assert_exported_absent(dest.path(), "cache");
    Ok(())
}

async fn llb_solve_cache_repeatability_test(docker: Docker) -> Result<(), Error> {
    let container_name = "bollard_llb_phase4_repeat";
    let dest1 = tempfile::tempdir()?;
    let def = cache_proof_def()?;

    solve_to_dir(
        &docker,
        def.clone(),
        dest1.path(),
        HashMap::new(),
        Some(container_name),
    )
    .await?;
    let proof1 = std::fs::read_to_string(dest1.path().join("cache-proof"))?;
    let n1: u64 = proof1.trim().parse().unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let dest2 = tempfile::tempdir()?;
    solve_to_dir(
        &docker,
        def,
        dest2.path(),
        HashMap::new(),
        Some(container_name),
    )
    .await?;
    let proof2 = std::fs::read_to_string(dest2.path().join("cache-proof"))?;
    let n2: u64 = proof2.trim().parse().unwrap();

    assert_eq!(
        n1, n2,
        "expected cache reuse to produce identical timestamp; got {n1} then {n2}"
    );
    Ok(())
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_mkfile() {
    connect_to_docker_and_run!(llb_solve_mkfile_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_golden_symlink() {
    connect_to_docker_and_run!(llb_solve_golden_symlink_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_golden_image() {
    connect_to_docker_and_run!(llb_solve_golden_image_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_golden_file_operations() {
    connect_to_docker_and_run!(llb_solve_golden_file_operations_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_scratch_mkfile() {
    connect_to_docker_and_run!(llb_solve_scratch_mkfile_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_file_operations() {
    connect_to_docker_and_run!(llb_solve_file_operations_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_image_exec() {
    connect_to_docker_and_run!(llb_solve_image_exec_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_file_secret() {
    connect_to_docker_and_run!(llb_solve_file_secret_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_env_secret() {
    connect_to_docker_and_run!(llb_solve_env_secret_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_merge_cache() {
    connect_to_docker_and_run!(llb_solve_merge_cache_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_cache_repeatability() {
    connect_to_docker_and_run!(llb_solve_cache_repeatability_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn bollard_llb_definition_to_pb_boundary() {
    use bollard_llb::{mkfile, scratch, FileOpts, MarshalOpts};

    let def = scratch()
        .unwrap()
        .file(mkfile("/hello", 0o644, b"world"), FileOpts::new())
        .unwrap()
        .marshal(MarshalOpts::linux_amd64())
        .unwrap()
        .to_pb();

    let _request =
        DefinitionSolveRequest::new(def, DefinitionExporter::Local(PathBuf::from("/out")));
}
