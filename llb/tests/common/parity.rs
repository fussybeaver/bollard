//! Shared comparator helpers for Go/Rust LLB parity tests.
//!
//! This module is intentionally test-only. It validates decoded `pb::Definition`
//! graphs and compares them field-by-field so that Phase 1 failures are
//! unambiguous.

#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use bollard_buildkit_proto::pb;
use prost::Message;
use sha2::{Digest as Sha2Digest, Sha256};

const VERTEX_PREFIX: &str = "vertex:";

/// A single diagnostic produced by validation or comparison.
#[derive(Debug)]
pub struct ParityDiagnostic {
    pub fixture: &'static str,
    pub category: &'static str,
    pub side: Option<&'static str>,
    pub vertex: Option<usize>,
    pub rust_digest: Option<String>,
    pub go_digest: Option<String>,
    pub path: String,
    pub rust_value: String,
    pub go_value: String,
    pub provenance: String,
}

impl fmt::Display for ParityDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "[{}] fixture={} path={}",
            self.category, self.fixture, self.path
        )?;
        if let Some(side) = self.side {
            writeln!(f, "  side={}", side)?;
        }
        if let Some(vertex) = self.vertex {
            writeln!(f, "  vertex={}", vertex)?;
        }
        if let Some(dgst) = &self.rust_digest {
            writeln!(f, "  rust_digest={}", dgst)?;
        }
        if let Some(dgst) = &self.go_digest {
            writeln!(f, "  go_digest={}", dgst)?;
        }
        writeln!(f, "  rust_value={}", self.rust_value)?;
        writeln!(f, "  go_value={}", self.go_value)?;
        write!(f, "  provenance={}", self.provenance)
    }
}

/// Short-hand for a computation that may produce many diagnostics.
pub type ParityResult<T> = Result<T, Vec<ParityDiagnostic>>;

/// Complete inventory of one side of a parity comparison.
#[derive(Debug, Clone)]
pub struct Inventory {
    pub def: pb::Definition,
    pub ops: Vec<pb::Op>,
    pub digests: Vec<String>,
    pub digest_to_index: HashMap<String, usize>,
}

/// Compute the SHA-256 digest of raw bytes in the `sha256:<hex>` form used by
/// BuildKit.
pub fn compute_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

/// Build an inventory from a decoded definition, checking that every vertex
/// can be decoded and that digests are unique.
pub fn build_inventory(
    def: pb::Definition,
    fixture: &'static str,
    side: &'static str,
    provenance: &str,
) -> ParityResult<Inventory> {
    let mut ops = Vec::with_capacity(def.def.len());
    let mut digests = Vec::with_capacity(def.def.len());
    let mut digest_to_index = HashMap::new();
    let mut diagnostics = Vec::new();

    for (i, bytes) in def.def.iter().enumerate() {
        let dgst = compute_digest(bytes);
        match pb::Op::decode(bytes.as_slice()) {
            Ok(op) => ops.push(op),
            Err(e) => diagnostics.push(ParityDiagnostic {
                fixture,
                category: "decode",
                side: Some(side),
                vertex: Some(i),
                rust_digest: None,
                go_digest: None,
                path: format!("def[{}]", i),
                rust_value: String::new(),
                go_value: e.to_string(),
                provenance: provenance.to_string(),
            }),
        }

        if digest_to_index.contains_key(&dgst) {
            diagnostics.push(ParityDiagnostic {
                fixture,
                category: "duplicate_digest",
                side: Some(side),
                vertex: Some(i),
                rust_digest: None,
                go_digest: None,
                path: format!("def[{}]", i),
                rust_value: dgst.clone(),
                go_value: dgst.clone(),
                provenance: provenance.to_string(),
            });
        } else {
            digest_to_index.insert(dgst.clone(), i);
        }
        digests.push(dgst);
    }

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    Ok(Inventory {
        def,
        ops,
        digests,
        digest_to_index,
    })
}

/// Validate the structural integrity of a single definition.
///
/// Returns an empty vector if the definition is valid. Diagnostics carry the
/// provided `side` ("rust" or "go") so the final report identifies which side
/// failed validation.
pub fn validate_inventory(
    inv: &Inventory,
    fixture: &'static str,
    side: &'static str,
    provenance: &str,
) -> Vec<ParityDiagnostic> {
    let mut diagnostics = Vec::new();

    if inv.def.def.is_empty() {
        return diagnostics;
    }

    // Every vertex must have a metadata entry and vice versa.
    for (i, dgst) in inv.digests.iter().enumerate() {
        if !inv.def.metadata.contains_key(dgst) {
            diagnostics.push(ParityDiagnostic {
                fixture,
                category: "metadata_missing",
                side: Some(side),
                vertex: Some(i),
                rust_digest: None,
                go_digest: None,
                path: format!("metadata[{}]", dgst),
                rust_value: String::new(),
                go_value: "missing metadata entry".to_string(),
                provenance: provenance.to_string(),
            });
        }
    }
    for key in inv.def.metadata.keys() {
        if !inv.digest_to_index.contains_key(key) {
            diagnostics.push(ParityDiagnostic {
                fixture,
                category: "metadata_orphan",
                side: Some(side),
                vertex: None,
                rust_digest: None,
                go_digest: None,
                path: format!("metadata[{}]", key),
                rust_value: "orphan key".to_string(),
                go_value: String::new(),
                provenance: provenance.to_string(),
            });
        }
    }

    let last_idx = inv.def.def.len() - 1;
    let last_op = &inv.ops[last_idx];

    // Wrapper checks.
    if last_op.op.is_some() {
        diagnostics.push(ParityDiagnostic {
            fixture,
            category: "wrapper_variant",
            side: Some(side),
            vertex: Some(last_idx),
            rust_digest: None,
            go_digest: None,
            path: "op.variant".to_string(),
            rust_value: format!("{:?}", last_op.op),
            go_value: "None".to_string(),
            provenance: provenance.to_string(),
        });
    }
    if last_op.inputs.len() != 1 {
        diagnostics.push(ParityDiagnostic {
            fixture,
            category: "wrapper_input_count",
            side: Some(side),
            vertex: Some(last_idx),
            rust_digest: None,
            go_digest: None,
            path: "inputs.len".to_string(),
            rust_value: last_op.inputs.len().to_string(),
            go_value: "1".to_string(),
            provenance: provenance.to_string(),
        });
    } else {
        let input = &last_op.inputs[0];
        if input.index != 0 {
            diagnostics.push(ParityDiagnostic {
                fixture,
                category: "wrapper_input_index",
                side: Some(side),
                vertex: Some(last_idx),
                rust_digest: None,
                go_digest: None,
                path: "inputs[0].index".to_string(),
                rust_value: input.index.to_string(),
                go_value: "0".to_string(),
                provenance: provenance.to_string(),
            });
        }
        diagnostics.extend(check_input(
            inv,
            fixture,
            side,
            provenance,
            last_idx,
            0,
            input,
            "inputs[0]",
        ));
    }

    // Topology and input/output validity for every vertex.
    for i in 0..inv.ops.len() {
        let op = &inv.ops[i];
        for (j, input) in op.inputs.iter().enumerate() {
            diagnostics.extend(check_input(
                inv,
                fixture,
                side,
                provenance,
                i,
                j,
                input,
                &format!("inputs[{}]", j),
            ));
        }
    }

    diagnostics
}

#[allow(clippy::too_many_arguments)]
fn check_input(
    inv: &Inventory,
    fixture: &'static str,
    side: &'static str,
    provenance: &str,
    consumer: usize,
    _input_index: usize,
    input: &pb::Input,
    path: &str,
) -> Vec<ParityDiagnostic> {
    let mut diagnostics = Vec::new();

    if input.index < 0 {
        diagnostics.push(ParityDiagnostic {
            fixture,
            category: "input_index_negative",
            side: Some(side),
            vertex: Some(consumer),
            rust_digest: None,
            go_digest: None,
            path: format!("{}.index", path),
            rust_value: input.index.to_string(),
            go_value: "non-negative".to_string(),
            provenance: provenance.to_string(),
        });
    }

    match inv.digest_to_index.get(&input.digest) {
        Some(&producer) => {
            if producer >= consumer {
                diagnostics.push(ParityDiagnostic {
                    fixture,
                    category: "input_order",
                    side: Some(side),
                    vertex: Some(consumer),
                    rust_digest: None,
                    go_digest: None,
                    path: format!("{}.digest", path),
                    rust_value: format!("vertex {} ({})", producer, input.digest),
                    go_value: format!("producer must precede vertex {}", consumer),
                    provenance: provenance.to_string(),
                });
            }
            if !is_valid_output_index(&inv.ops[producer], input.index) {
                diagnostics.push(ParityDiagnostic {
                    fixture,
                    category: "input_output_index",
                    side: Some(side),
                    vertex: Some(consumer),
                    rust_digest: None,
                    go_digest: None,
                    path: format!("{}.index", path),
                    rust_value: input.index.to_string(),
                    go_value: format!(
                        "valid outputs for producer {} are {:?}",
                        producer,
                        output_indices(&inv.ops[producer])
                    ),
                    provenance: provenance.to_string(),
                });
            }
        }
        None => {
            diagnostics.push(ParityDiagnostic {
                fixture,
                category: "input_digest_missing",
                side: Some(side),
                vertex: Some(consumer),
                rust_digest: None,
                go_digest: None,
                path: format!("{}.digest", path),
                rust_value: input.digest.clone(),
                go_value: "no vertex with this digest".to_string(),
                provenance: provenance.to_string(),
            });
        }
    }

    diagnostics
}

/// Returns the set of output indices that an op exposes to its consumers.
fn output_indices(op: &pb::Op) -> Vec<i64> {
    let mut set: HashSet<i64> = HashSet::new();
    match &op.op {
        Some(pb::op::Op::Exec(exec)) => {
            set.insert(0);
            for m in &exec.mounts {
                if m.output >= 0 {
                    set.insert(m.output);
                }
            }
        }
        Some(pb::op::Op::File(file)) => {
            for a in &file.actions {
                if a.output >= 0 {
                    set.insert(a.output);
                }
            }
            if set.is_empty() {
                set.insert(0);
            }
        }
        Some(pb::op::Op::Build(_)) | Some(pb::op::Op::Diff(_)) => {
            set.insert(0);
        }
        _ => {
            set.insert(0);
        }
    }
    let mut indices: Vec<i64> = set.into_iter().collect();
    indices.sort_unstable();
    indices
}

fn is_valid_output_index(op: &pb::Op, index: i64) -> bool {
    output_indices(op).contains(&index)
}

/// Main entry point used by parity tests.
pub fn assert_go_parity(
    fixture: &'static str,
    golden_bytes: &[u8],
    build: impl FnOnce() -> bollard_llb::Definition,
    provenance: String,
) {
    let rust_def = build().to_pb();
    let go_def = pb::Definition::decode(golden_bytes).expect("Go golden protobuf should decode");

    let rust_inv = build_inventory(rust_def, fixture, "rust", &provenance)
        .unwrap_or_else(|d| panic_inventory_failure(fixture, &provenance, d));
    let go_inv = build_inventory(go_def, fixture, "go", &provenance)
        .unwrap_or_else(|d| panic_inventory_failure(fixture, &provenance, d));

    let mut diagnostics = validate_inventory(&rust_inv, fixture, "rust", &provenance);
    diagnostics.extend(validate_inventory(&go_inv, fixture, "go", &provenance));

    if diagnostics.is_empty() {
        diagnostics.extend(compare_definitions(
            &rust_inv,
            &go_inv,
            fixture,
            &provenance,
        ));
    }

    if !diagnostics.is_empty() {
        panic!("{}\n", format_diagnostics(&diagnostics));
    }
}

fn panic_inventory_failure(
    fixture: &str,
    provenance: &str,
    diagnostics: Vec<ParityDiagnostic>,
) -> ! {
    panic!(
        "failed to build inventory for fixture={} provenance={}: {}",
        fixture,
        provenance,
        format_diagnostics(&diagnostics)
    );
}

fn format_diagnostics(diagnostics: &[ParityDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compare two already-valid definitions. Digest strings are resolved to graph
/// positions before field comparison, so the two sides are compared by topology
/// and protobuf content rather than by the exact `sha256:...` identifiers.
pub fn compare_definitions(
    left: &Inventory,
    right: &Inventory,
    fixture: &'static str,
    provenance: &str,
) -> Vec<ParityDiagnostic> {
    let mut diagnostics = Vec::new();

    let left_kinds: Vec<&'static str> = left.ops.iter().map(op_kind).collect();
    let right_kinds: Vec<&'static str> = right.ops.iter().map(op_kind).collect();

    if left.ops.len() != right.ops.len() {
        diagnostics.push(ParityDiagnostic {
            fixture,
            category: "topology_count",
            side: None,
            vertex: None,
            rust_digest: None,
            go_digest: None,
            path: "def.len".to_string(),
            rust_value: left.ops.len().to_string(),
            go_value: right.ops.len().to_string(),
            provenance: provenance.to_string(),
        });
        // Still report the first op-kind mismatch so the fixture is easier to
        // diagnose.
        for i in 0..left_kinds.len().min(right_kinds.len()) {
            if left_kinds[i] != right_kinds[i] {
                diagnostics.push(ParityDiagnostic {
                    fixture,
                    category: "topology_kind",
                    side: None,
                    vertex: Some(i),
                    rust_digest: Some(left.digests[i].clone()),
                    go_digest: Some(right.digests[i].clone()),
                    path: "op.variant".to_string(),
                    rust_value: left_kinds[i].to_string(),
                    go_value: right_kinds[i].to_string(),
                    provenance: provenance.to_string(),
                });
                break;
            }
        }
        return diagnostics;
    }

    for i in 0..left_kinds.len() {
        if left_kinds[i] != right_kinds[i] {
            diagnostics.push(ParityDiagnostic {
                fixture,
                category: "topology_kind",
                side: None,
                vertex: Some(i),
                rust_digest: Some(left.digests[i].clone()),
                go_digest: Some(right.digests[i].clone()),
                path: "op.variant".to_string(),
                rust_value: left_kinds[i].to_string(),
                go_value: right_kinds[i].to_string(),
                provenance: provenance.to_string(),
            });
        }
    }

    if !diagnostics.is_empty() {
        return diagnostics;
    }

    let left_canonical = canonicalize_ops(left);
    let right_canonical = canonicalize_ops(right);

    // Decoded semantic comparison.
    for i in 0..left_canonical.len() {
        let l = &left_canonical[i];
        let r = &right_canonical[i];
        if l != r {
            let mut diffs = Vec::new();
            diff_op("op", l, r, &mut diffs);
            for (path, rust_value, go_value) in diffs {
                diagnostics.push(ParityDiagnostic {
                    fixture,
                    category: "semantic",
                    side: None,
                    vertex: Some(i),
                    rust_digest: Some(left.digests[i].clone()),
                    go_digest: Some(right.digests[i].clone()),
                    path,
                    rust_value,
                    go_value,
                    provenance: provenance.to_string(),
                });
            }
        }
    }

    // Metadata comparison.
    let left_meta = metadata_by_index(left);
    let right_meta = metadata_by_index(right);
    let all_indices: HashSet<usize> = left_meta.keys().chain(right_meta.keys()).copied().collect();
    for i in all_indices {
        match (left_meta.get(&i), right_meta.get(&i)) {
            (Some(l), Some(r)) => {
                let mut diffs = Vec::new();
                diff_op_metadata(&format!("metadata[{}]", i), l, r, &mut diffs);
                for (path, rust_value, go_value) in diffs {
                    diagnostics.push(ParityDiagnostic {
                        fixture,
                        category: "metadata",
                        side: None,
                        vertex: Some(i),
                        rust_digest: Some(left.digests[i].clone()),
                        go_digest: Some(right.digests[i].clone()),
                        path,
                        rust_value,
                        go_value,
                        provenance: provenance.to_string(),
                    });
                }
            }
            (None, Some(_)) => diagnostics.push(ParityDiagnostic {
                fixture,
                category: "metadata",
                side: None,
                vertex: Some(i),
                rust_digest: Some(left.digests[i].clone()),
                go_digest: Some(right.digests[i].clone()),
                path: format!("metadata[{}]", i),
                rust_value: "missing".to_string(),
                go_value: "present".to_string(),
                provenance: provenance.to_string(),
            }),
            (Some(_), None) => diagnostics.push(ParityDiagnostic {
                fixture,
                category: "metadata",
                side: None,
                vertex: Some(i),
                rust_digest: Some(left.digests[i].clone()),
                go_digest: Some(right.digests[i].clone()),
                path: format!("metadata[{}]", i),
                rust_value: "present".to_string(),
                go_value: "missing".to_string(),
                provenance: provenance.to_string(),
            }),
            (None, None) => {}
        }
    }

    // Source-map comparison.
    let left_source = canonicalize_source(left);
    let right_source = canonicalize_source(right);
    let mut source_diffs = Vec::new();
    diff_source("source", &left_source, &right_source, &mut source_diffs);
    for (path, rust_value, go_value) in source_diffs {
        diagnostics.push(ParityDiagnostic {
            fixture,
            category: "source",
            side: None,
            vertex: None,
            rust_digest: None,
            go_digest: None,
            path,
            rust_value,
            go_value,
            provenance: provenance.to_string(),
        });
    }

    // Raw-byte comparison, reported only for vertices whose decoded semantic
    // content already matches. Byte differences here are therefore encoder or
    // digest-string differences, not semantic differences.
    for i in 0..left_canonical.len() {
        if left_canonical[i] == right_canonical[i] && left.def.def[i] != right.def.def[i] {
            diagnostics.push(ParityDiagnostic {
                fixture,
                category: "wire",
                side: None,
                vertex: Some(i),
                rust_digest: Some(left.digests[i].clone()),
                go_digest: Some(right.digests[i].clone()),
                path: "raw_bytes".to_string(),
                rust_value: format!("{} bytes", left.def.def[i].len()),
                go_value: format!("{} bytes", right.def.def[i].len()),
                provenance: provenance.to_string(),
            });
        }
    }

    diagnostics
}

fn op_kind(op: &pb::Op) -> &'static str {
    match &op.op {
        Some(pb::op::Op::Source(_)) => "source",
        Some(pb::op::Op::Exec(_)) => "exec",
        Some(pb::op::Op::File(_)) => "file",
        Some(pb::op::Op::Build(_)) => "build",
        Some(pb::op::Op::Merge(_)) => "merge",
        Some(pb::op::Op::Diff(_)) => "diff",
        None => "wrapper",
    }
}

fn canonical_label(inv: &Inventory, digest: &str) -> String {
    inv.digest_to_index
        .get(digest)
        .map(|&i| format!("{}{}", VERTEX_PREFIX, i))
        .unwrap_or_else(|| digest.to_string())
}

fn canonicalize_ops(inv: &Inventory) -> Vec<pb::Op> {
    let mut ops = inv.ops.clone();
    for op in &mut ops {
        for input in &mut op.inputs {
            input.digest = canonical_label(inv, &input.digest);
        }
    }
    ops
}

fn metadata_by_index(inv: &Inventory) -> BTreeMap<usize, pb::OpMetadata> {
    let mut map = BTreeMap::new();
    for (k, v) in &inv.def.metadata {
        if let Some(&idx) = inv.digest_to_index.get(k) {
            map.insert(idx, v.clone());
        }
    }
    map
}

fn canonicalize_source(inv: &Inventory) -> Option<pb::Source> {
    inv.def.source.as_ref().map(|s| {
        let mut locations = BTreeMap::new();
        for (k, v) in &s.locations {
            locations.insert(canonical_label(inv, k), v.clone());
        }
        pb::Source {
            locations,
            infos: s.infos.clone(),
        }
    })
}

fn note_diff<T: PartialEq + fmt::Debug>(
    path: &str,
    left: &T,
    right: &T,
    diffs: &mut Vec<(String, String, String)>,
) {
    if left != right {
        diffs.push((
            path.to_string(),
            format!("{:?}", left),
            format!("{:?}", right),
        ));
    }
}

fn diff_op(path: &str, left: &pb::Op, right: &pb::Op, diffs: &mut Vec<(String, String, String)>) {
    if left.inputs.len() != right.inputs.len() {
        diffs.push((
            format!("{}.inputs.len", path),
            left.inputs.len().to_string(),
            right.inputs.len().to_string(),
        ));
    } else {
        for (i, (l, r)) in left.inputs.iter().zip(right.inputs.iter()).enumerate() {
            let p = format!("{}.inputs[{}]", path, i);
            note_diff(&format!("{}.index", p), &l.index, &r.index, diffs);
            note_diff(&format!("{}.digest", p), &l.digest, &r.digest, diffs);
        }
    }
    diff_option_platform(
        &format!("{}.platform", path),
        &left.platform,
        &right.platform,
        diffs,
    );
    diff_option_constraints(
        &format!("{}.constraints", path),
        &left.constraints,
        &right.constraints,
        diffs,
    );

    match (&left.op, &right.op) {
        (Some(pb::op::Op::Exec(l)), Some(pb::op::Op::Exec(r))) => {
            diff_exec_op(&format!("{}.exec", path), l, r, diffs);
        }
        (Some(pb::op::Op::Source(l)), Some(pb::op::Op::Source(r))) => {
            diff_source_op(&format!("{}.source", path), l, r, diffs);
        }
        (Some(pb::op::Op::File(l)), Some(pb::op::Op::File(r))) => {
            diff_file_op(&format!("{}.file", path), l, r, diffs);
        }
        (Some(pb::op::Op::Merge(l)), Some(pb::op::Op::Merge(r))) => {
            diff_merge_op(&format!("{}.merge", path), l, r, diffs);
        }
        (Some(pb::op::Op::Build(l)), Some(pb::op::Op::Build(r))) => {
            note_diff(&format!("{}.build", path), l, r, diffs);
        }
        (Some(pb::op::Op::Diff(l)), Some(pb::op::Op::Diff(r))) => {
            note_diff(&format!("{}.diff", path), l, r, diffs);
        }
        (None, None) => {}
        _ => {
            diffs.push((
                format!("{}.op.variant", path),
                format!("{:?}", left.op),
                format!("{:?}", right.op),
            ));
        }
    }
}

fn diff_exec_op(
    path: &str,
    left: &pb::ExecOp,
    right: &pb::ExecOp,
    diffs: &mut Vec<(String, String, String)>,
) {
    if let (Some(l), Some(r)) = (&left.meta, &right.meta) {
        diff_meta(&format!("{}.meta", path), l, r, diffs);
    } else {
        note_diff(&format!("{}.meta", path), &left.meta, &right.meta, diffs);
    }
    if left.mounts.len() != right.mounts.len() {
        diffs.push((
            format!("{}.mounts.len", path),
            left.mounts.len().to_string(),
            right.mounts.len().to_string(),
        ));
    } else {
        for (i, (l, r)) in left.mounts.iter().zip(right.mounts.iter()).enumerate() {
            diff_mount(&format!("{}.mounts[{}]", path, i), l, r, diffs);
        }
    }
    note_diff(
        &format!("{}.network", path),
        &left.network,
        &right.network,
        diffs,
    );
    note_diff(
        &format!("{}.security", path),
        &left.security,
        &right.security,
        diffs,
    );
    diff_repeated(
        &format!("{}.secretenv", path),
        &left.secretenv,
        &right.secretenv,
        diffs,
    );
    diff_repeated(
        &format!("{}.cdi_devices", path),
        &left.cdi_devices,
        &right.cdi_devices,
        diffs,
    );
}

fn diff_meta(
    path: &str,
    left: &pb::Meta,
    right: &pb::Meta,
    diffs: &mut Vec<(String, String, String)>,
) {
    note_diff(&format!("{}.args", path), &left.args, &right.args, diffs);
    note_diff(&format!("{}.env", path), &left.env, &right.env, diffs);
    note_diff(&format!("{}.cwd", path), &left.cwd, &right.cwd, diffs);
    note_diff(&format!("{}.user", path), &left.user, &right.user, diffs);
    diff_option_proxy_env(
        &format!("{}.proxy_env", path),
        &left.proxy_env,
        &right.proxy_env,
        diffs,
    );
    diff_repeated(
        &format!("{}.extra_hosts", path),
        &left.extra_hosts,
        &right.extra_hosts,
        diffs,
    );
    note_diff(
        &format!("{}.hostname", path),
        &left.hostname,
        &right.hostname,
        diffs,
    );
    diff_repeated(
        &format!("{}.ulimit", path),
        &left.ulimit,
        &right.ulimit,
        diffs,
    );
    note_diff(
        &format!("{}.cgroup_parent", path),
        &left.cgroup_parent,
        &right.cgroup_parent,
        diffs,
    );
    note_diff(
        &format!("{}.remove_mount_stubs_recursive", path),
        &left.remove_mount_stubs_recursive,
        &right.remove_mount_stubs_recursive,
        diffs,
    );
    note_diff(
        &format!("{}.valid_exit_codes", path),
        &left.valid_exit_codes,
        &right.valid_exit_codes,
        diffs,
    );
}

fn diff_mount(
    path: &str,
    left: &pb::Mount,
    right: &pb::Mount,
    diffs: &mut Vec<(String, String, String)>,
) {
    note_diff(&format!("{}.input", path), &left.input, &right.input, diffs);
    note_diff(
        &format!("{}.selector", path),
        &left.selector,
        &right.selector,
        diffs,
    );
    note_diff(&format!("{}.dest", path), &left.dest, &right.dest, diffs);
    note_diff(
        &format!("{}.output", path),
        &left.output,
        &right.output,
        diffs,
    );
    note_diff(
        &format!("{}.readonly", path),
        &left.readonly,
        &right.readonly,
        diffs,
    );
    note_diff(
        &format!("{}.mount_type", path),
        &left.mount_type,
        &right.mount_type,
        diffs,
    );
    diff_option_tmpfs_opt(
        &format!("{}.tmpfs_opt", path),
        &left.tmpfs_opt,
        &right.tmpfs_opt,
        diffs,
    );
    diff_option_cache_opt(
        &format!("{}.cache_opt", path),
        &left.cache_opt,
        &right.cache_opt,
        diffs,
    );
    diff_option_secret_opt(
        &format!("{}.secret_opt", path),
        &left.secret_opt,
        &right.secret_opt,
        diffs,
    );
    diff_option_ssh_opt(
        &format!("{}.ssh_opt", path),
        &left.ssh_opt,
        &right.ssh_opt,
        diffs,
    );
    note_diff(
        &format!("{}.result_id", path),
        &left.result_id,
        &right.result_id,
        diffs,
    );
    note_diff(
        &format!("{}.content_cache", path),
        &left.content_cache,
        &right.content_cache,
        diffs,
    );
}

fn diff_source_op(
    path: &str,
    left: &pb::SourceOp,
    right: &pb::SourceOp,
    diffs: &mut Vec<(String, String, String)>,
) {
    note_diff(
        &format!("{}.identifier", path),
        &left.identifier,
        &right.identifier,
        diffs,
    );
    diff_map(&format!("{}.attrs", path), &left.attrs, &right.attrs, diffs);
}

fn diff_file_op(
    path: &str,
    left: &pb::FileOp,
    right: &pb::FileOp,
    diffs: &mut Vec<(String, String, String)>,
) {
    if left.actions.len() != right.actions.len() {
        diffs.push((
            format!("{}.actions.len", path),
            left.actions.len().to_string(),
            right.actions.len().to_string(),
        ));
        return;
    }
    for (i, (l, r)) in left.actions.iter().zip(right.actions.iter()).enumerate() {
        diff_file_action(&format!("{}.actions[{}]", path, i), l, r, diffs);
    }
}

fn diff_file_action(
    path: &str,
    left: &pb::FileAction,
    right: &pb::FileAction,
    diffs: &mut Vec<(String, String, String)>,
) {
    note_diff(&format!("{}.input", path), &left.input, &right.input, diffs);
    note_diff(
        &format!("{}.secondary_input", path),
        &left.secondary_input,
        &right.secondary_input,
        diffs,
    );
    note_diff(
        &format!("{}.output", path),
        &left.output,
        &right.output,
        diffs,
    );
    match (&left.action, &right.action) {
        (Some(pb::file_action::Action::Copy(l)), Some(pb::file_action::Action::Copy(r))) => {
            diff_file_action_copy(&format!("{}.action.copy", path), l, r, diffs);
        }
        (Some(pb::file_action::Action::Mkfile(l)), Some(pb::file_action::Action::Mkfile(r))) => {
            diff_file_action_mkfile(&format!("{}.action.mkfile", path), l, r, diffs);
        }
        (Some(pb::file_action::Action::Mkdir(l)), Some(pb::file_action::Action::Mkdir(r))) => {
            diff_file_action_mkdir(&format!("{}.action.mkdir", path), l, r, diffs);
        }
        (Some(pb::file_action::Action::Rm(l)), Some(pb::file_action::Action::Rm(r))) => {
            diff_file_action_rm(&format!("{}.action.rm", path), l, r, diffs);
        }
        (Some(pb::file_action::Action::Symlink(l)), Some(pb::file_action::Action::Symlink(r))) => {
            diff_file_action_symlink(&format!("{}.action.symlink", path), l, r, diffs);
        }
        (None, None) => {}
        _ => {
            diffs.push((
                format!("{}.action.variant", path),
                format!("{:?}", left.action),
                format!("{:?}", right.action),
            ));
        }
    }
}

fn diff_file_action_copy(
    path: &str,
    left: &pb::FileActionCopy,
    right: &pb::FileActionCopy,
    diffs: &mut Vec<(String, String, String)>,
) {
    note_diff(&format!("{}.src", path), &left.src, &right.src, diffs);
    note_diff(&format!("{}.dest", path), &left.dest, &right.dest, diffs);
    diff_option_chown_opt(&format!("{}.owner", path), &left.owner, &right.owner, diffs);
    note_diff(&format!("{}.mode", path), &left.mode, &right.mode, diffs);
    note_diff(
        &format!("{}.follow_symlink", path),
        &left.follow_symlink,
        &right.follow_symlink,
        diffs,
    );
    note_diff(
        &format!("{}.dir_copy_contents", path),
        &left.dir_copy_contents,
        &right.dir_copy_contents,
        diffs,
    );
    note_diff(
        &format!("{}.attempt_unpack_docker_compatibility", path),
        &left.attempt_unpack_docker_compatibility,
        &right.attempt_unpack_docker_compatibility,
        diffs,
    );
    note_diff(
        &format!("{}.create_dest_path", path),
        &left.create_dest_path,
        &right.create_dest_path,
        diffs,
    );
    note_diff(
        &format!("{}.allow_wildcard", path),
        &left.allow_wildcard,
        &right.allow_wildcard,
        diffs,
    );
    note_diff(
        &format!("{}.allow_empty_wildcard", path),
        &left.allow_empty_wildcard,
        &right.allow_empty_wildcard,
        diffs,
    );
    note_diff(
        &format!("{}.timestamp", path),
        &left.timestamp,
        &right.timestamp,
        diffs,
    );
    note_diff(
        &format!("{}.include_patterns", path),
        &left.include_patterns,
        &right.include_patterns,
        diffs,
    );
    note_diff(
        &format!("{}.exclude_patterns", path),
        &left.exclude_patterns,
        &right.exclude_patterns,
        diffs,
    );
    note_diff(
        &format!("{}.always_replace_existing_dest_paths", path),
        &left.always_replace_existing_dest_paths,
        &right.always_replace_existing_dest_paths,
        diffs,
    );
    note_diff(
        &format!("{}.mode_str", path),
        &left.mode_str,
        &right.mode_str,
        diffs,
    );
    note_diff(
        &format!("{}.required_paths", path),
        &left.required_paths,
        &right.required_paths,
        diffs,
    );
}

fn diff_file_action_mkfile(
    path: &str,
    left: &pb::FileActionMkFile,
    right: &pb::FileActionMkFile,
    diffs: &mut Vec<(String, String, String)>,
) {
    note_diff(&format!("{}.path", path), &left.path, &right.path, diffs);
    note_diff(&format!("{}.mode", path), &left.mode, &right.mode, diffs);
    note_diff(&format!("{}.data", path), &left.data, &right.data, diffs);
    diff_option_chown_opt(&format!("{}.owner", path), &left.owner, &right.owner, diffs);
    note_diff(
        &format!("{}.timestamp", path),
        &left.timestamp,
        &right.timestamp,
        diffs,
    );
}

fn diff_file_action_mkdir(
    path: &str,
    left: &pb::FileActionMkDir,
    right: &pb::FileActionMkDir,
    diffs: &mut Vec<(String, String, String)>,
) {
    note_diff(&format!("{}.path", path), &left.path, &right.path, diffs);
    note_diff(&format!("{}.mode", path), &left.mode, &right.mode, diffs);
    note_diff(
        &format!("{}.make_parents", path),
        &left.make_parents,
        &right.make_parents,
        diffs,
    );
    diff_option_chown_opt(&format!("{}.owner", path), &left.owner, &right.owner, diffs);
    note_diff(
        &format!("{}.timestamp", path),
        &left.timestamp,
        &right.timestamp,
        diffs,
    );
}

fn diff_file_action_rm(
    path: &str,
    left: &pb::FileActionRm,
    right: &pb::FileActionRm,
    diffs: &mut Vec<(String, String, String)>,
) {
    note_diff(&format!("{}.path", path), &left.path, &right.path, diffs);
    note_diff(
        &format!("{}.allow_not_found", path),
        &left.allow_not_found,
        &right.allow_not_found,
        diffs,
    );
    note_diff(
        &format!("{}.allow_wildcard", path),
        &left.allow_wildcard,
        &right.allow_wildcard,
        diffs,
    );
}

fn diff_file_action_symlink(
    path: &str,
    left: &pb::FileActionSymlink,
    right: &pb::FileActionSymlink,
    diffs: &mut Vec<(String, String, String)>,
) {
    note_diff(
        &format!("{}.oldpath", path),
        &left.oldpath,
        &right.oldpath,
        diffs,
    );
    note_diff(
        &format!("{}.newpath", path),
        &left.newpath,
        &right.newpath,
        diffs,
    );
    diff_option_chown_opt(&format!("{}.owner", path), &left.owner, &right.owner, diffs);
    note_diff(
        &format!("{}.timestamp", path),
        &left.timestamp,
        &right.timestamp,
        diffs,
    );
}

fn diff_merge_op(
    path: &str,
    left: &pb::MergeOp,
    right: &pb::MergeOp,
    diffs: &mut Vec<(String, String, String)>,
) {
    if left.inputs.len() != right.inputs.len() {
        diffs.push((
            format!("{}.inputs.len", path),
            left.inputs.len().to_string(),
            right.inputs.len().to_string(),
        ));
        return;
    }
    for (i, (l, r)) in left.inputs.iter().zip(right.inputs.iter()).enumerate() {
        note_diff(
            &format!("{}.inputs[{}].input", path, i),
            &l.input,
            &r.input,
            diffs,
        );
    }
}

fn diff_option_platform(
    path: &str,
    left: &Option<pb::Platform>,
    right: &Option<pb::Platform>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => {
            note_diff(
                &format!("{}.architecture", path),
                &l.architecture,
                &r.architecture,
                diffs,
            );
            note_diff(&format!("{}.os", path), &l.os, &r.os, diffs);
            note_diff(&format!("{}.variant", path), &l.variant, &r.variant, diffs);
            note_diff(
                &format!("{}.os_version", path),
                &l.os_version,
                &r.os_version,
                diffs,
            );
            note_diff(
                &format!("{}.os_features", path),
                &l.os_features,
                &r.os_features,
                diffs,
            );
        }
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left),
                format!("{:?}", right),
            ));
        }
    }
}

fn diff_option_constraints(
    path: &str,
    left: &Option<pb::WorkerConstraints>,
    right: &Option<pb::WorkerConstraints>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => {
            note_diff(&format!("{}.filter", path), &l.filter, &r.filter, diffs);
        }
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left),
                format!("{:?}", right),
            ));
        }
    }
}

fn diff_option_proxy_env(
    path: &str,
    left: &Option<pb::ProxyEnv>,
    right: &Option<pb::ProxyEnv>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => {
            note_diff(
                &format!("{}.http_proxy", path),
                &l.http_proxy,
                &r.http_proxy,
                diffs,
            );
            note_diff(
                &format!("{}.https_proxy", path),
                &l.https_proxy,
                &r.https_proxy,
                diffs,
            );
            note_diff(
                &format!("{}.ftp_proxy", path),
                &l.ftp_proxy,
                &r.ftp_proxy,
                diffs,
            );
            note_diff(
                &format!("{}.no_proxy", path),
                &l.no_proxy,
                &r.no_proxy,
                diffs,
            );
            note_diff(
                &format!("{}.all_proxy", path),
                &l.all_proxy,
                &r.all_proxy,
                diffs,
            );
        }
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left),
                format!("{:?}", right),
            ));
        }
    }
}

fn diff_option_tmpfs_opt(
    path: &str,
    left: &Option<pb::TmpfsOpt>,
    right: &Option<pb::TmpfsOpt>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => {
            note_diff(&format!("{}.size", path), &l.size, &r.size, diffs);
        }
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left),
                format!("{:?}", right),
            ));
        }
    }
}

fn diff_option_cache_opt(
    path: &str,
    left: &Option<pb::CacheOpt>,
    right: &Option<pb::CacheOpt>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => {
            note_diff(&format!("{}.id", path), &l.id, &r.id, diffs);
            note_diff(&format!("{}.sharing", path), &l.sharing, &r.sharing, diffs);
        }
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left),
                format!("{:?}", right),
            ));
        }
    }
}

fn diff_option_secret_opt(
    path: &str,
    left: &Option<pb::SecretOpt>,
    right: &Option<pb::SecretOpt>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => {
            note_diff(&format!("{}.id", path), &l.id, &r.id, diffs);
            note_diff(&format!("{}.uid", path), &l.uid, &r.uid, diffs);
            note_diff(&format!("{}.gid", path), &l.gid, &r.gid, diffs);
            note_diff(&format!("{}.mode", path), &l.mode, &r.mode, diffs);
            note_diff(
                &format!("{}.optional", path),
                &l.optional,
                &r.optional,
                diffs,
            );
        }
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left),
                format!("{:?}", right),
            ));
        }
    }
}

fn diff_option_ssh_opt(
    path: &str,
    left: &Option<pb::SshOpt>,
    right: &Option<pb::SshOpt>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => {
            note_diff(&format!("{}.id", path), &l.id, &r.id, diffs);
            note_diff(&format!("{}.uid", path), &l.uid, &r.uid, diffs);
            note_diff(&format!("{}.gid", path), &l.gid, &r.gid, diffs);
            note_diff(&format!("{}.mode", path), &l.mode, &r.mode, diffs);
            note_diff(
                &format!("{}.optional", path),
                &l.optional,
                &r.optional,
                diffs,
            );
        }
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left),
                format!("{:?}", right),
            ));
        }
    }
}

fn diff_option_chown_opt(
    path: &str,
    left: &Option<pb::ChownOpt>,
    right: &Option<pb::ChownOpt>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => {
            diff_option_user_opt(&format!("{}.user", path), &l.user, &r.user, diffs);
            diff_option_user_opt(&format!("{}.group", path), &l.group, &r.group, diffs);
        }
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left),
                format!("{:?}", right),
            ));
        }
    }
}

fn diff_option_user_opt(
    path: &str,
    left: &Option<pb::UserOpt>,
    right: &Option<pb::UserOpt>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => match (&l.user, &r.user) {
            (Some(pb::user_opt::User::ByName(ln)), Some(pb::user_opt::User::ByName(rn))) => {
                note_diff(&format!("{}.by_name.name", path), &ln.name, &rn.name, diffs);
                note_diff(
                    &format!("{}.by_name.input", path),
                    &ln.input,
                    &rn.input,
                    diffs,
                );
            }
            (Some(pb::user_opt::User::ById(lid)), Some(pb::user_opt::User::ById(rid))) => {
                note_diff(&format!("{}.by_id", path), lid, rid, diffs);
            }
            (None, None) => {}
            _ => {
                diffs.push((
                    format!("{}.user", path),
                    format!("{:?}", l.user),
                    format!("{:?}", r.user),
                ));
            }
        },
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left),
                format!("{:?}", right),
            ));
        }
    }
}

fn diff_option_export_cache(
    path: &str,
    left: &Option<pb::ExportCache>,
    right: &Option<pb::ExportCache>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => {
            note_diff(&format!("{}.value", path), &l.value, &r.value, diffs);
        }
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left),
                format!("{:?}", right),
            ));
        }
    }
}

fn diff_option_progress_group(
    path: &str,
    left: &Option<pb::ProgressGroup>,
    right: &Option<pb::ProgressGroup>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => {
            note_diff(&format!("{}.id", path), &l.id, &r.id, diffs);
            note_diff(&format!("{}.name", path), &l.name, &r.name, diffs);
            note_diff(&format!("{}.weak", path), &l.weak, &r.weak, diffs);
        }
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left),
                format!("{:?}", right),
            ));
        }
    }
}

fn diff_op_metadata(
    path: &str,
    left: &pb::OpMetadata,
    right: &pb::OpMetadata,
    diffs: &mut Vec<(String, String, String)>,
) {
    note_diff(
        &format!("{}.ignore_cache", path),
        &left.ignore_cache,
        &right.ignore_cache,
        diffs,
    );
    diff_map(
        &format!("{}.description", path),
        &left.description,
        &right.description,
        diffs,
    );
    diff_option_export_cache(
        &format!("{}.export_cache", path),
        &left.export_cache,
        &right.export_cache,
        diffs,
    );
    diff_map(&format!("{}.caps", path), &left.caps, &right.caps, diffs);
    diff_option_progress_group(
        &format!("{}.progress_group", path),
        &left.progress_group,
        &right.progress_group,
        diffs,
    );
}

fn diff_source(
    path: &str,
    left: &Option<pb::Source>,
    right: &Option<pb::Source>,
    diffs: &mut Vec<(String, String, String)>,
) {
    match (left, right) {
        (Some(l), Some(r)) => {
            diff_map(
                &format!("{}.locations", path),
                &l.locations,
                &r.locations,
                diffs,
            );
            diff_repeated(&format!("{}.infos", path), &l.infos, &r.infos, diffs);
        }
        (None, None) => {}
        _ => {
            diffs.push((
                path.to_string(),
                format!("{:?}", left.is_some()),
                format!("{:?}", right.is_some()),
            ));
        }
    }
}

fn diff_map<V: PartialEq + fmt::Debug>(
    path: &str,
    left: &BTreeMap<String, V>,
    right: &BTreeMap<String, V>,
    diffs: &mut Vec<(String, String, String)>,
) {
    for (k, lv) in left {
        match right.get(k) {
            Some(rv) => {
                if lv != rv {
                    diffs.push((
                        format!("{}[{:?}]", path, k),
                        format!("{:?}", lv),
                        format!("{:?}", rv),
                    ));
                }
            }
            None => {
                diffs.push((
                    format!("{}[{:?}]", path, k),
                    format!("{:?}", lv),
                    "missing".to_string(),
                ));
            }
        }
    }
    for (k, rv) in right {
        if !left.contains_key(k) {
            diffs.push((
                format!("{}[{:?}]", path, k),
                "missing".to_string(),
                format!("{:?}", rv),
            ));
        }
    }
}

fn diff_repeated<T: PartialEq + fmt::Debug>(
    path: &str,
    left: &[T],
    right: &[T],
    diffs: &mut Vec<(String, String, String)>,
) {
    if left.len() != right.len() {
        diffs.push((
            format!("{}.len", path),
            left.len().to_string(),
            right.len().to_string(),
        ));
        return;
    }
    for (i, (l, r)) in left.iter().zip(right.iter()).enumerate() {
        note_diff(&format!("{}[{}]", path, i), l, r, diffs);
    }
}

/// Re-encode a definition through prost to obtain a canonical local byte form.
///
/// This is useful for mutation tests that need to compare a mutated graph with
/// an unmutated graph using the same encoder.
pub fn prost_roundtrip(def: &pb::Definition) -> pb::Definition {
    let mut buf = Vec::new();
    def.encode(&mut buf).expect("Definition should encode");
    pb::Definition::decode(buf.as_slice()).expect("Definition should round-trip")
}

/// Apply a per-vertex mutation to a definition and then rebuild all digests and
/// references so the graph remains internally consistent.
///
/// The mutation closure receives each decoded `pb::Op` and its vertex index. After
/// mutations, every vertex is re-encoded with prost, its digest is recomputed,
/// and all input references, metadata keys, and source-map keys are remapped.
/// This preserves topological order while changing the requested field.
pub fn remap_after_mutation<F>(def: &mut pb::Definition, mut mutate: F)
where
    F: FnMut(&mut pb::Op, usize),
{
    let mut old_to_new: HashMap<String, String> = HashMap::new();
    let mut new_bytes: Vec<Vec<u8>> = Vec::with_capacity(def.def.len());

    for (i, bytes) in def.def.iter().enumerate() {
        let mut op = pb::Op::decode(bytes.as_slice()).expect("vertex should decode");
        mutate(&mut op, i);

        for input in &mut op.inputs {
            if let Some(new_dgst) = old_to_new.get(&input.digest) {
                input.digest.clone_from(new_dgst);
            }
        }

        let mut buf = Vec::new();
        op.encode(&mut buf).expect("vertex should encode");
        let new_dgst = compute_digest(&buf);
        old_to_new.insert(compute_digest(bytes), new_dgst);
        new_bytes.push(buf);
    }

    def.def = new_bytes;

    let mut new_metadata: BTreeMap<String, pb::OpMetadata> = BTreeMap::new();
    for (k, v) in def.metadata.iter() {
        let new_k = old_to_new.get(k).cloned().unwrap_or_else(|| k.clone());
        new_metadata.insert(new_k, v.clone());
    }
    def.metadata = new_metadata;

    if let Some(source) = def.source.as_mut() {
        let mut new_locations: BTreeMap<String, pb::Locations> = BTreeMap::new();
        for (k, v) in source.locations.iter() {
            let new_k = old_to_new.get(k).cloned().unwrap_or_else(|| k.clone());
            new_locations.insert(new_k, v.clone());
        }
        source.locations = new_locations;
    }
}

/// Replace a single input digest with a value that does not exist in the graph.
pub fn break_input_digest(def: &mut pb::Definition, consumer_vertex: usize, input_index: usize) {
    let mut op = pb::Op::decode(def.def[consumer_vertex].as_slice()).expect("vertex should decode");
    if input_index < op.inputs.len() {
        op.inputs[input_index].digest =
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string();
    }
    let mut buf = Vec::new();
    op.encode(&mut buf).expect("vertex should encode");
    def.def[consumer_vertex] = buf;
}
