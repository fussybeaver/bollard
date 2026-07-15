//! Snapshot tests for marshalled LLB graphs.
//!
//! Each test constructs a known graph, marshals it with a fixed platform
//! (`linux/amd64`), decodes the serialized `pb::Op` vertices, and compares the
//! result against an insta golden snapshot. Because the crate uses content
//! digests and deterministic protobuf map ordering (BTreeMap), the output is
//! stable across runs.

use std::collections::BTreeMap;

use bollard_buildkit_proto::pb;
use bollard_llb::{
    copy, image, merge, mkdir, mkfile, rm, scratch, shlex, symlink, AddSecret, CacheSharingMode,
    FileOpts, Local, MarshalOpts, MergeOpts, State,
};
use prost::Message;

/// A snapshot-friendly view of a [`bollard_llb::Definition`].
///
/// The fields are read through the derived [`Debug`] impl by insta; they are
/// not accessed directly, which triggers dead-code analysis.
#[derive(Debug)]
#[allow(dead_code)]
struct SnapshotDefinition {
    root: String,
    ops: Vec<pb::Op>,
    metadata: BTreeMap<String, pb::OpMetadata>,
}

impl From<&bollard_llb::Definition> for SnapshotDefinition {
    fn from(def: &bollard_llb::Definition) -> Self {
        Self {
            root: def.root.to_string(),
            ops: def
                .def
                .iter()
                .map(|bytes| pb::Op::decode(bytes.as_slice()).unwrap())
                .collect(),
            metadata: def.metadata.clone(),
        }
    }
}

#[test]
fn snapshot_image_run() {
    let def = image("alpine:latest")
        .run(shlex("echo hello"))
        .root()
        .marshal(MarshalOpts::linux_amd64())
        .unwrap();
    insta::assert_debug_snapshot!(SnapshotDefinition::from(&def));
}

#[test]
fn snapshot_merge() {
    let def = merge(
        vec![image("alpine:latest"), image("busybox:latest")],
        MergeOpts::new(),
    )
    .marshal(MarshalOpts::linux_amd64())
    .unwrap();
    insta::assert_debug_snapshot!(SnapshotDefinition::from(&def));
}

#[test]
fn snapshot_copy_all_flags() {
    let base = image("alpine:latest");
    let src = image("busybox:latest");
    let action = copy(src, "/src", "/dst")
        .with_create_dest_path(true)
        .with_follow_symlinks(true)
        .with_copy_dir_contents_only(true)
        .with_allow_wildcard(true)
        .with_allow_empty_wildcard(true)
        .with_exclude_pattern("*.tmp");
    let def = base
        .file(action, FileOpts::new())
        .marshal(MarshalOpts::linux_amd64())
        .unwrap();
    insta::assert_debug_snapshot!(SnapshotDefinition::from(&def));
}

#[test]
fn snapshot_mkdir_parents() {
    let def = scratch()
        .file(mkdir("/tmp", 0o755).with_parents(true), FileOpts::new())
        .marshal(MarshalOpts::linux_amd64())
        .unwrap();
    insta::assert_debug_snapshot!(SnapshotDefinition::from(&def));
}

#[test]
fn snapshot_mkfile() {
    let def = scratch()
        .file(mkfile("/hello", 0o644, b"world"), FileOpts::new())
        .marshal(MarshalOpts::linux_amd64())
        .unwrap();
    insta::assert_debug_snapshot!(SnapshotDefinition::from(&def));
}

#[test]
fn snapshot_secret_as_env() {
    let def = image("alpine:latest")
        .run(shlex("cat /secrets/token"))
        .add_secret(
            "token",
            AddSecret {
                id: String::new(),
                as_env: true,
                env_name: Some("TOKEN".into()),
                target: None,
                optional: false,
            },
        )
        .root()
        .marshal(MarshalOpts::linux_amd64())
        .unwrap();
    insta::assert_debug_snapshot!(SnapshotDefinition::from(&def));
}

#[test]
fn snapshot_local_all_attrs() {
    let def = State::from(
        Local::new("context")
            .with_follow_paths(["src"])
            .with_session_id("sess")
            .with_shared_key_hint("hint")
            .with_unique_id("unique"),
    )
    .marshal(MarshalOpts::linux_amd64())
    .unwrap();
    insta::assert_debug_snapshot!(SnapshotDefinition::from(&def));
}

#[test]
fn snapshot_cache_mount_shared() {
    let def = image("alpine:latest")
        .run(shlex("echo hello"))
        .add_mount_cache("/cache", "cache-id", CacheSharingMode::Shared)
        .root()
        .marshal(MarshalOpts::linux_amd64())
        .unwrap();
    insta::assert_debug_snapshot!(SnapshotDefinition::from(&def));
}

#[test]
fn snapshot_cache_mount_locked() {
    let def = image("alpine:latest")
        .run(shlex("echo hello"))
        .add_mount_cache("/cache", "cache-id", CacheSharingMode::Locked)
        .root()
        .marshal(MarshalOpts::linux_amd64())
        .unwrap();
    insta::assert_debug_snapshot!(SnapshotDefinition::from(&def));
}

#[test]
fn snapshot_file_operations_chain() {
    let base = scratch().file(mkdir("/app", 0o755).with_parents(true), FileOpts::new());

    let with_config = base.clone().file(
        mkfile("/app/config.toml", 0o644, b"[server]\nhost = \"0.0.0.0\"\n"),
        FileOpts::new(),
    );

    let with_symlink = with_config.clone().file(
        symlink("/app/config.toml", "/app/current-config"),
        FileOpts::new(),
    );

    let with_copy = with_symlink.clone().file(
        copy(with_symlink, "/app/config.toml", "/app/config.toml.bak").with_create_dest_path(true),
        FileOpts::new(),
    );

    let def = with_copy
        .file(rm("/app/current-config"), FileOpts::new())
        .marshal(MarshalOpts::linux_amd64())
        .unwrap();
    insta::assert_debug_snapshot!(SnapshotDefinition::from(&def));
}
