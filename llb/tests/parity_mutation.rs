//! Mutation tests for the Phase 1 Go/Rust LLB comparator.
//!
//! These tests take committed Go goldens, perturb a single field, and assert
//! that the comparator reports the expected diagnostic. They ensure the
//! comparator is not just checking vertex counts.

use bollard_buildkit_proto::pb;
use prost::Message;

mod common;

fn provenance(name: &str) -> String {
    format!("mutation={}", name)
}

fn baseline_image_run() -> pb::Definition {
    let bytes = include_bytes!("../testdata/golden/image_run.llb.pb");
    let def = pb::Definition::decode(bytes.as_slice()).unwrap();
    common::parity::prost_roundtrip(&def)
}

fn baseline_cache_mount_shared() -> pb::Definition {
    let bytes = include_bytes!("../testdata/golden/cache_mount_shared.llb.pb");
    let def = pb::Definition::decode(bytes.as_slice()).unwrap();
    common::parity::prost_roundtrip(&def)
}

fn baseline_mkfile() -> pb::Definition {
    let bytes = include_bytes!("../testdata/golden/mkfile.llb.pb");
    let def = pb::Definition::decode(bytes.as_slice()).unwrap();
    common::parity::prost_roundtrip(&def)
}

fn compare_and_find(
    fixture: &'static str,
    left: &pb::Definition,
    right: &pb::Definition,
    expected_category: &str,
    expected_path_suffix: &str,
) -> bool {
    let left_inv =
        common::parity::build_inventory(left.clone(), fixture, "left", &provenance(fixture))
            .expect("left inventory should build");
    let right_inv =
        common::parity::build_inventory(right.clone(), fixture, "right", &provenance(fixture))
            .expect("right inventory should build");

    let mut diagnostics =
        common::parity::validate_inventory(&left_inv, fixture, "left", &provenance(fixture));
    diagnostics.extend(common::parity::validate_inventory(
        &right_inv,
        fixture,
        "right",
        &provenance(fixture),
    ));

    if diagnostics.is_empty() {
        diagnostics = common::parity::compare_definitions(
            &left_inv,
            &right_inv,
            fixture,
            &provenance(fixture),
        );
    }

    let found = diagnostics
        .iter()
        .any(|d| d.category == expected_category && d.path.ends_with(expected_path_suffix));
    if !found {
        eprintln!("diagnostics for {}:\n{:#?}", fixture, diagnostics);
    }
    found
}

#[test]
fn mutation_input_digest_is_detected() {
    let mut def = baseline_image_run();
    common::parity::break_input_digest(&mut def, 1, 0);

    let inv = common::parity::build_inventory(
        def,
        "input_digest",
        "mutated",
        &provenance("input_digest"),
    )
    .expect("inventory should still build");
    let diagnostics = common::parity::validate_inventory(
        &inv,
        "input_digest",
        "mutated",
        &provenance("input_digest"),
    );

    assert!(
        diagnostics
            .iter()
            .any(|d| d.category == "input_digest_missing" && d.path == "inputs[0].digest"),
        "expected missing-input digest diagnostic, got: {:?}",
        diagnostics
    );
}

#[test]
fn mutation_platform_is_detected() {
    let left = baseline_image_run();
    let mut right = left.clone();
    common::parity::remap_after_mutation(&mut right, |op, i| {
        if i == 0 {
            if let Some(platform) = op.platform.as_mut() {
                platform.architecture = "mips64".to_string();
            }
        }
    });

    assert!(
        compare_and_find(
            "platform",
            &left,
            &right,
            "semantic",
            "platform.architecture",
        ),
        "platform architecture change should be detected"
    );
}

#[test]
fn mutation_mount_type_is_detected() {
    let left = baseline_cache_mount_shared();
    let mut right = left.clone();
    common::parity::remap_after_mutation(&mut right, |op, i| {
        if i == 1 {
            if let Some(pb::op::Op::Exec(exec)) = op.op.as_mut() {
                // mounts[0] is the rootfs bind mount; mounts[1] is the cache mount.
                if exec.mounts.len() > 1 {
                    exec.mounts[1].mount_type = pb::MountType::Bind as i32;
                    exec.mounts[1].cache_opt = None;
                }
            }
        }
    });

    assert!(
        compare_and_find(
            "mount_type",
            &left,
            &right,
            "semantic",
            "mounts[1].mount_type",
        ),
        "mount type change should be detected"
    );
}

#[test]
fn mutation_file_mode_is_detected() {
    let left = baseline_mkfile();
    let mut right = left.clone();
    common::parity::remap_after_mutation(&mut right, |op, i| {
        if i == 0 {
            if let Some(pb::op::Op::File(file)) = op.op.as_mut() {
                if let Some(pb::file_action::Action::Mkfile(mkfile)) =
                    file.actions[0].action.as_mut()
                {
                    mkfile.mode = 0o777;
                }
            }
        }
    });

    assert!(
        compare_and_find("file_mode", &left, &right, "semantic", "action.mkfile.mode",),
        "file mode change should be detected"
    );
}

#[test]
fn mutation_capability_is_detected() {
    let left = baseline_image_run();
    let mut right = left.clone();

    // The exec vertex is at index 1 in this simple fixture.
    let exec_digest = common::parity::compute_digest(&right.def[1]);
    if let Some(meta) = right.metadata.get_mut(&exec_digest) {
        meta.caps.remove("exec.meta.base");
    }

    assert!(
        compare_and_find(
            "capability",
            &left,
            &right,
            "metadata",
            "caps[\"exec.meta.base\"]",
        ),
        "capability change should be detected"
    );
}

#[test]
fn mutation_source_attr_is_detected() {
    let left = baseline_image_run();
    let mut right = left.clone();
    common::parity::remap_after_mutation(&mut right, |op, i| {
        if i == 0 {
            if let Some(pb::op::Op::Source(source)) = op.op.as_mut() {
                source
                    .attrs
                    .insert("image.resolvemode".to_string(), "pull".to_string());
            }
        }
    });

    assert!(
        compare_and_find(
            "source_attr",
            &left,
            &right,
            "semantic",
            "attrs[\"image.resolvemode\"]",
        ),
        "source attribute change should be detected"
    );
}

#[test]
fn mutation_source_map_presence_is_detected() {
    let left = baseline_image_run();
    let mut right = left.clone();
    right.source = None;

    assert!(
        compare_and_find("source_map", &left, &right, "source", "source",),
        "source-map presence change should be detected"
    );
}
