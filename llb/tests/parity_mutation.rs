//! Mutation tests for the Go/Rust LLB comparator.
//!
//! These tests perturb one semantic field in a committed Go definition and
//! verify that the comparator reports the specific changed field.

use bollard_buildkit_proto::pb;
use prost::Message;

mod common;

fn provenance(name: &str) -> String {
    format!("mutation={name}")
}

fn golden(name: &str) -> pb::Definition {
    let bytes: &[u8] = match name {
        "image_run" => include_bytes!("../testdata/golden/image_run.llb.pb"),
        "cache_mount_shared" => include_bytes!("../testdata/golden/cache_mount_shared.llb.pb"),
        "mkfile" => include_bytes!("../testdata/golden/mkfile.llb.pb"),
        _ => panic!("unknown mutation fixture {name}"),
    };
    common::parity::prost_roundtrip(
        &pb::Definition::decode(bytes).expect("golden definition should decode"),
    )
}

fn compare_and_find(
    fixture: &'static str,
    left: &pb::Definition,
    right: &pb::Definition,
    expected_category: &str,
    expected_path_suffix: &str,
) -> bool {
    let provenance = provenance(fixture);
    let left_inv = common::parity::build_inventory(left.clone(), fixture, "left", &provenance)
        .expect("left inventory should build");
    let right_inv = common::parity::build_inventory(right.clone(), fixture, "right", &provenance)
        .expect("right inventory should build");

    let mut diagnostics =
        common::parity::validate_inventory(&left_inv, fixture, "left", &provenance);
    diagnostics.extend(common::parity::validate_inventory(
        &right_inv,
        fixture,
        "right",
        &provenance,
    ));
    if diagnostics.is_empty() {
        diagnostics =
            common::parity::compare_definitions(&left_inv, &right_inv, fixture, &provenance);
    }

    let found = diagnostics.iter().any(|diagnostic| {
        diagnostic.category == expected_category && diagnostic.path.ends_with(expected_path_suffix)
    });
    if !found {
        eprintln!("diagnostics for {fixture}:\n{diagnostics:#?}");
    }
    found
}

#[test]
fn mutation_input_digest_is_detected() {
    let mut definition = golden("image_run");
    common::parity::break_input_digest(&mut definition, 1, 0);

    let provenance = provenance("input_digest");
    let inventory =
        common::parity::build_inventory(definition, "input_digest", "mutated", &provenance)
            .expect("mutated inventory should build");
    let diagnostics =
        common::parity::validate_inventory(&inventory, "input_digest", "mutated", &provenance);

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.category == "input_digest_missing" && diagnostic.path == "inputs[0].digest"
        }),
        "expected missing-input digest diagnostic, got: {diagnostics:?}"
    );
}

#[test]
fn mutation_platform_is_detected() {
    let left = golden("image_run");
    let mut right = left.clone();
    common::parity::remap_after_mutation(&mut right, |op, index| {
        if index == 0 {
            if let Some(platform) = op.platform.as_mut() {
                platform.architecture = String::from("mips64");
            }
        }
    });

    assert!(compare_and_find(
        "platform",
        &left,
        &right,
        "semantic",
        "platform.architecture",
    ));
}

#[test]
fn mutation_mount_type_is_detected() {
    let left = golden("cache_mount_shared");
    let mut right = left.clone();
    common::parity::remap_after_mutation(&mut right, |op, index| {
        if index == 1 {
            if let Some(pb::op::Op::Exec(exec)) = op.op.as_mut() {
                if exec.mounts.len() > 1 {
                    exec.mounts[1].mount_type = pb::MountType::Bind as i32;
                    exec.mounts[1].cache_opt = None;
                }
            }
        }
    });

    assert!(compare_and_find(
        "mount_type",
        &left,
        &right,
        "semantic",
        "mounts[1].mount_type",
    ));
}

#[test]
fn mutation_file_mode_is_detected() {
    let left = golden("mkfile");
    let mut right = left.clone();
    common::parity::remap_after_mutation(&mut right, |op, index| {
        if index == 0 {
            if let Some(pb::op::Op::File(file)) = op.op.as_mut() {
                if let Some(pb::file_action::Action::Mkfile(mkfile)) =
                    file.actions[0].action.as_mut()
                {
                    mkfile.mode = 0o777;
                }
            }
        }
    });

    assert!(compare_and_find(
        "file_mode",
        &left,
        &right,
        "semantic",
        "action.mkfile.mode",
    ));
}

#[test]
fn mutation_capability_is_detected() {
    let left = golden("image_run");
    let mut right = left.clone();
    let exec_digest = common::parity::compute_digest(&right.def[1]);
    if let Some(metadata) = right.metadata.get_mut(&exec_digest) {
        metadata.caps.remove("exec.meta.base");
    }

    assert!(compare_and_find(
        "capability",
        &left,
        &right,
        "metadata",
        "caps[\"exec.meta.base\"]",
    ));
}

#[test]
fn mutation_source_attr_is_detected() {
    let left = golden("image_run");
    let mut right = left.clone();
    common::parity::remap_after_mutation(&mut right, |op, index| {
        if index == 0 {
            if let Some(pb::op::Op::Source(source)) = op.op.as_mut() {
                source
                    .attrs
                    .insert(String::from("image.resolvemode"), String::from("pull"));
            }
        }
    });

    assert!(compare_and_find(
        "source_attr",
        &left,
        &right,
        "semantic",
        "attrs[\"image.resolvemode\"]",
    ));
}

#[test]
fn mutation_source_map_presence_is_detected() {
    let left = golden("image_run");
    let mut right = left.clone();
    right.source = None;

    assert!(compare_and_find(
        "source_map",
        &left,
        &right,
        "source",
        "source",
    ));
}
