//! Focused assertions for platform and worker-constraint propagation.

use bollard_buildkit_proto::pb;
use bollard_llb::{merge, shlex, Image, MarshalOpts, MergeOpts, Platform, State};
use prost::Message;

mod common;

fn ops(definition: &bollard_llb::Definition) -> Vec<pb::Op> {
    definition
        .def
        .iter()
        .map(|bytes| pb::Op::decode(bytes.as_slice()).expect("definition op should decode"))
        .collect()
}

fn source_op<'a>(ops: &'a [pb::Op], image: &str) -> &'a pb::Op {
    ops.iter()
        .find(|op| match &op.op {
            Some(pb::op::Op::Source(source)) => source.identifier.ends_with(image),
            _ => false,
        })
        .unwrap_or_else(|| panic!("missing image source {image}"))
}

fn exec_op<'a>(ops: &'a [pb::Op], arg: &str) -> &'a pb::Op {
    ops.iter()
        .find(|op| match &op.op {
            Some(pb::op::Op::Exec(exec)) => exec
                .meta
                .as_ref()
                .is_some_and(|meta| meta.args.iter().any(|value| value == arg)),
            _ => false,
        })
        .unwrap_or_else(|| panic!("missing exec argument {arg}"))
}

fn assert_platform(op: &pb::Op, platform: Platform) {
    assert_eq!(op.platform.as_ref(), Some(&platform.into()));
}

fn assert_definition_valid(definition: &bollard_llb::Definition, fixture: &'static str) {
    let inventory = common::parity::build_inventory(
        definition.to_pb(),
        fixture,
        "rust",
        "phase-c-platform-assertions",
    )
    .expect("definition inventory should build");
    let diagnostics = common::parity::validate_inventory(
        &inventory,
        fixture,
        "rust",
        "phase-c-platform-assertions",
    );
    assert!(
        diagnostics.is_empty(),
        "invalid definition: {diagnostics:#?}"
    );
}

fn mixed_definition() -> bollard_llb::Definition {
    let arm_v6 = Platform::new("linux", "arm").with_variant("v6");
    let subgraph = State::from(
        Image::new("image2:latest")
            .unwrap()
            .with_platform(arm_v6)
            .unwrap(),
    )
    .run(shlex("cmd-sub"))
    .root()
    .unwrap();

    State::from(Image::new("image1:latest").unwrap())
        .run(shlex("cmd-main"))
        .add_mount("/mnt", subgraph)
        .root()
        .unwrap()
        .marshal(MarshalOpts::linux_amd64())
        .unwrap()
}

#[test]
fn operation_local_platform_overrides_marshal_default() {
    let definition = State::from(
        Image::new("local:latest")
            .unwrap()
            .with_platform(Platform::LINUX_ARM64)
            .unwrap(),
    )
    .run(shlex("local-op"))
    .root()
    .unwrap()
    .marshal(MarshalOpts::linux_amd64())
    .unwrap();
    let decoded = ops(&definition);

    assert_platform(source_op(&decoded, "local:latest"), Platform::LINUX_ARM64);
    assert_platform(exec_op(&decoded, "local-op"), Platform::LINUX_ARM64);
    assert!(decoded
        .iter()
        .any(|op| op.op.is_none() && op.platform.is_none() && op.constraints.is_none()));
}

#[test]
fn state_platform_applies_to_subsequent_exec_only() {
    let definition = State::from(Image::new("state:latest").unwrap())
        .with_platform(Platform::LINUX_ARM64)
        .run(shlex("state-op"))
        .root()
        .unwrap()
        .marshal(MarshalOpts::linux_amd64())
        .unwrap();
    let decoded = ops(&definition);

    assert_platform(source_op(&decoded, "state:latest"), Platform::LINUX_AMD64);
    assert_platform(exec_op(&decoded, "state-op"), Platform::LINUX_ARM64);
}

#[test]
fn image_platform_is_retained_by_source_and_descendant_exec() {
    let definition = State::from(
        Image::new("image-local:latest")
            .unwrap()
            .with_platform(Platform::LINUX_ARM64)
            .unwrap(),
    )
    .run(shlex("image-local-op"))
    .root()
    .unwrap()
    .marshal(MarshalOpts::linux_amd64())
    .unwrap();
    let decoded = ops(&definition);

    assert_platform(
        source_op(&decoded, "image-local:latest"),
        Platform::LINUX_ARM64,
    );
    assert_platform(exec_op(&decoded, "image-local-op"), Platform::LINUX_ARM64);
}

#[test]
fn mixed_platform_graph_keeps_each_vertex_platform() {
    let definition = mixed_definition();
    let decoded = ops(&definition);

    assert_platform(source_op(&decoded, "image1:latest"), Platform::LINUX_AMD64);
    assert_platform(exec_op(&decoded, "cmd-main"), Platform::LINUX_AMD64);
    let arm_v6 = Platform::new("linux", "arm").with_variant("v6");
    assert_platform(source_op(&decoded, "image2:latest"), arm_v6.clone());
    assert_platform(exec_op(&decoded, "cmd-sub"), arm_v6);
    assert_definition_valid(&definition, "mixed_platform_assertions");
}

fn branched_definition(changed_platform: Platform) -> bollard_llb::Definition {
    let changed = State::from(
        Image::new("changed:latest")
            .unwrap()
            .with_platform(changed_platform)
            .unwrap(),
    )
    .run(shlex("changed-op"))
    .root()
    .unwrap();
    let stable = State::from(Image::new("stable:latest").unwrap())
        .run(shlex("stable-op"))
        .root()
        .unwrap();

    merge(vec![changed, stable], MergeOpts::new())
        .unwrap()
        .marshal(MarshalOpts::linux_amd64())
        .unwrap()
}

#[test]
fn local_platform_change_only_changes_affected_branch_and_ancestors() {
    let arm64 = ops(&branched_definition(Platform::LINUX_ARM64));
    let arm_v7 = ops(&branched_definition(Platform::LINUX_ARM_V7));

    let changed_source_arm64 = source_op(&arm64, "changed:latest");
    let changed_source_arm_v7 = source_op(&arm_v7, "changed:latest");
    assert_ne!(
        common::parity::compute_digest(&encode(changed_source_arm64)),
        common::parity::compute_digest(&encode(changed_source_arm_v7))
    );

    let changed_exec_arm64 = exec_op(&arm64, "changed-op");
    let changed_exec_arm_v7 = exec_op(&arm_v7, "changed-op");
    assert_ne!(
        common::parity::compute_digest(&encode(changed_exec_arm64)),
        common::parity::compute_digest(&encode(changed_exec_arm_v7))
    );

    let stable_source_arm64 = source_op(&arm64, "stable:latest");
    let stable_source_arm_v7 = source_op(&arm_v7, "stable:latest");
    assert_eq!(encode(stable_source_arm64), encode(stable_source_arm_v7));
    let stable_exec_arm64 = exec_op(&arm64, "stable-op");
    let stable_exec_arm_v7 = exec_op(&arm_v7, "stable-op");
    assert_eq!(encode(stable_exec_arm64), encode(stable_exec_arm_v7));

    let merge_arm64 = arm64
        .iter()
        .find(|op| matches!(op.op, Some(pb::op::Op::Merge(_))))
        .expect("expected merge op");
    let merge_arm_v7 = arm_v7
        .iter()
        .find(|op| matches!(op.op, Some(pb::op::Op::Merge(_))))
        .expect("expected merge op");
    assert_ne!(encode(merge_arm64), encode(merge_arm_v7));

    let wrapper_arm64 = arm64
        .iter()
        .find(|op| op.op.is_none())
        .expect("expected wrapper op");
    let wrapper_arm_v7 = arm_v7
        .iter()
        .find(|op| op.op.is_none())
        .expect("expected wrapper op");
    assert_ne!(encode(wrapper_arm64), encode(wrapper_arm_v7));
}

fn encode(op: &pb::Op) -> Vec<u8> {
    let mut bytes = Vec::new();
    op.encode(&mut bytes).expect("operation should encode");
    bytes
}

fn shared_definition(platform: Platform) -> bollard_llb::Definition {
    let shared = State::from(Image::new("shared:latest").unwrap())
        .run(shlex("shared-op"))
        .root()
        .unwrap();
    State::from(Image::new("main:latest").unwrap())
        .run(shlex("main-op"))
        .add_mount("/left", shared.clone())
        .add_mount("/right", shared)
        .root()
        .unwrap()
        .marshal(MarshalOpts::linux_amd64().with_platform(platform))
        .unwrap()
}

#[test]
fn shared_subgraph_is_reserialized_for_each_marshal_platform() {
    let amd64 = shared_definition(Platform::LINUX_AMD64);
    let arm64 = shared_definition(Platform::LINUX_ARM64);
    assert_definition_valid(&amd64, "shared_subgraph_amd64");
    assert_definition_valid(&arm64, "shared_subgraph_arm64");

    let amd64_ops = ops(&amd64);
    let arm64_ops = ops(&arm64);
    assert_ne!(
        encode(source_op(&amd64_ops, "shared:latest")),
        encode(source_op(&arm64_ops, "shared:latest"))
    );
    assert_ne!(
        encode(exec_op(&amd64_ops, "shared-op")),
        encode(exec_op(&arm64_ops, "shared-op"))
    );
}

#[test]
fn wrapper_has_no_platform_or_worker_constraints() {
    let definition = State::from(Image::new("wrapper:latest").unwrap())
        .run(shlex("wrapper-op"))
        .root()
        .unwrap()
        .marshal(
            MarshalOpts::linux_amd64()
                .with_platform(Platform::LINUX_ARM64)
                .with_worker_filter("worker.labels.worker=platform-test"),
        )
        .unwrap();
    let decoded = ops(&definition);
    let wrapper = decoded
        .iter()
        .find(|op| op.op.is_none())
        .expect("definition should contain a wrapper");

    assert_eq!(wrapper.platform, None);
    assert_eq!(wrapper.constraints, None);
}

#[test]
fn worker_constraints_are_present_on_real_operations_only() {
    let filter = "worker.labels.worker=platform-test".to_string();
    let definition = State::from(Image::new("worker:latest").unwrap())
        .run(shlex("worker-op"))
        .root()
        .unwrap()
        .marshal(MarshalOpts::linux_amd64().with_worker_filter(filter.clone()))
        .unwrap();

    for op in ops(&definition) {
        if op.op.is_none() {
            assert_eq!(op.constraints, None);
        } else {
            assert_eq!(
                op.constraints.as_ref().map(|c| &c.filter),
                Some(&vec![filter.clone()])
            );
        }
    }
}
