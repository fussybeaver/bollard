//! Human-readable and JSON dumping of LLB [`Definition`]s.
//!
//! The binary encoding of a definition is provided by
//! [`Definition::write_to`](crate::definition::Definition::write_to), which
//! matches Go's `llb.WriteTo(def, w)`. This module adds debugging-oriented
//! output formats:
//!
//! * [`dump_text`] — hand-rolled readable text, always available.
//! * `dump_json` — JSON Lines output similar to `buildctl debug dump-llb`,
//!   gated behind the `dump_json` feature.

use std::io::Write;

use bollard_buildkit_proto::pb;
use prost::Message;

use crate::definition::Definition;
use crate::error::LlbError;
use crate::marshal::sha256;

/// Write a human-readable text dump of `def` to `w`.
///
/// Output is one vertex at a time in the same order as `def.def` (children
/// before parents). No extra dependencies are required for this format.
pub fn dump_text<W: Write>(def: &Definition, w: &mut W) -> Result<(), LlbError> {
    for bytes in &def.def {
        let op = pb::Op::decode(bytes.as_slice()).map_err(|source| LlbError::Decode {
            op: "dump_text".to_string(),
            source,
        })?;
        let digest = sha256(bytes);
        let md = def.metadata.get(digest.as_str());
        dump_text_op(w, &digest, &op, md)?;
        writeln!(w)?;
    }
    Ok(())
}

fn dump_text_op<W: Write>(
    w: &mut W,
    digest: &crate::marshal::Digest,
    op: &pb::Op,
    md: Option<&pb::OpMetadata>,
) -> Result<(), LlbError> {
    let kind = op_kind(op);
    writeln!(w, "[{}] {}", digest, kind)?;

    write!(w, "  inputs: [")?;
    for (i, input) in op.inputs.iter().enumerate() {
        if i > 0 {
            write!(w, ", ")?;
        }
        write!(w, "{}:{}", input.digest, input.index)?;
    }
    writeln!(w, "]")?;

    match &op.op {
        Some(pb::op::Op::Exec(exec)) => dump_text_exec(w, exec)?,
        Some(pb::op::Op::Source(source)) => dump_text_source(w, source)?,
        Some(pb::op::Op::File(file)) => dump_text_file(w, file)?,
        Some(pb::op::Op::Merge(merge)) => dump_text_merge(w, merge)?,
        Some(pb::op::Op::Build(build)) => dump_text_build(w, build)?,
        Some(pb::op::Op::Diff(diff)) => dump_text_diff(w, diff)?,
        None => writeln!(w, "  (wrapper)")?,
    }

    if let Some(platform) = &op.platform {
        writeln!(w, "  platform: {}/{}", platform.os, platform.architecture)?;
    }
    if let Some(constraints) = &op.constraints {
        writeln!(w, "  constraints: {:?}", constraints.filter)?;
    }

    if let Some(md) = md {
        dump_text_metadata(w, md)?;
    }

    Ok(())
}

fn op_kind(op: &pb::Op) -> &'static str {
    match &op.op {
        Some(pb::op::Op::Exec(_)) => "exec",
        Some(pb::op::Op::Source(_)) => "source",
        Some(pb::op::Op::File(_)) => "file",
        Some(pb::op::Op::Build(_)) => "build",
        Some(pb::op::Op::Merge(_)) => "merge",
        Some(pb::op::Op::Diff(_)) => "diff",
        None => "wrapper",
    }
}

fn dump_text_exec<W: Write>(w: &mut W, exec: &pb::ExecOp) -> Result<(), LlbError> {
    writeln!(w, "  exec:")?;
    if let Some(meta) = &exec.meta {
        writeln!(w, "    args: {:?}", meta.args)?;
        if !meta.env.is_empty() {
            writeln!(w, "    env: {:?}", meta.env)?;
        }
        if !meta.cwd.is_empty() {
            writeln!(w, "    cwd: {}", meta.cwd)?;
        }
        if !meta.user.is_empty() {
            writeln!(w, "    user: {}", meta.user)?;
        }
        if !meta.hostname.is_empty() {
            writeln!(w, "    hostname: {}", meta.hostname)?;
        }
    }
    if !exec.mounts.is_empty() {
        writeln!(w, "    mounts:")?;
        for mount in &exec.mounts {
            writeln!(
                w,
                "      - input: {}, dest: {}, type: {}, output: {}",
                mount.input,
                mount.dest,
                mount_type_name(mount.mount_type),
                mount.output
            )?;
        }
    }
    if !exec.secretenv.is_empty() {
        writeln!(w, "    secretenv:")?;
        for secret in &exec.secretenv {
            writeln!(
                w,
                "      - id: {}, name: {}, optional: {}",
                secret.id, secret.name, secret.optional
            )?;
        }
    }
    writeln!(
        w,
        "    network: {}, security: {}",
        net_mode_name(exec.network),
        security_mode_name(exec.security)
    )?;
    Ok(())
}

fn dump_text_source<W: Write>(w: &mut W, source: &pb::SourceOp) -> Result<(), LlbError> {
    writeln!(w, "  source:")?;
    writeln!(w, "    identifier: {}", source.identifier)?;
    if !source.attrs.is_empty() {
        writeln!(w, "    attrs: {:?}", source.attrs)?;
    }
    Ok(())
}

fn dump_text_file<W: Write>(w: &mut W, file: &pb::FileOp) -> Result<(), LlbError> {
    writeln!(w, "  file:")?;
    for action in &file.actions {
        writeln!(
            w,
            "    - input: {}, secondary_input: {}, output: {}",
            action.input, action.secondary_input, action.output
        )?;
        match &action.action {
            Some(pb::file_action::Action::Copy(copy)) => {
                writeln!(
                    w,
                    "      copy: src={}, dest={}, mode={}",
                    copy.src, copy.dest, copy.mode
                )?;
            }
            Some(pb::file_action::Action::Mkfile(mkfile)) => {
                writeln!(
                    w,
                    "      mkfile: path={}, mode={}, bytes={}",
                    mkfile.path,
                    mkfile.mode,
                    mkfile.data.len()
                )?;
            }
            Some(pb::file_action::Action::Mkdir(mkdir)) => {
                writeln!(
                    w,
                    "      mkdir: path={}, mode={}, parents={}",
                    mkdir.path, mkdir.mode, mkdir.make_parents
                )?;
            }
            Some(pb::file_action::Action::Rm(rm)) => {
                writeln!(w, "      rm: path={}", rm.path)?;
            }
            Some(pb::file_action::Action::Symlink(symlink)) => {
                writeln!(
                    w,
                    "      symlink: oldpath={}, newpath={}",
                    symlink.oldpath, symlink.newpath
                )?;
            }
            None => {}
        }
    }
    Ok(())
}

fn dump_text_merge<W: Write>(w: &mut W, merge: &pb::MergeOp) -> Result<(), LlbError> {
    writeln!(w, "  merge:")?;
    for input in &merge.inputs {
        writeln!(w, "    - input: {}", input.input)?;
    }
    Ok(())
}

fn dump_text_build<W: Write>(w: &mut W, build: &pb::BuildOp) -> Result<(), LlbError> {
    writeln!(w, "  build:")?;
    writeln!(w, "    builder: {}", build.builder)?;
    if !build.inputs.is_empty() {
        writeln!(w, "    inputs: {:?}", build.inputs)?;
    }
    if !build.attrs.is_empty() {
        writeln!(w, "    attrs: {:?}", build.attrs)?;
    }
    Ok(())
}

fn dump_text_diff<W: Write>(w: &mut W, diff: &pb::DiffOp) -> Result<(), LlbError> {
    writeln!(w, "  diff:")?;
    if let Some(lower) = &diff.lower {
        writeln!(w, "    lower: {}", lower.input)?;
    }
    if let Some(upper) = &diff.upper {
        writeln!(w, "    upper: {}", upper.input)?;
    }
    Ok(())
}

fn dump_text_metadata<W: Write>(w: &mut W, md: &pb::OpMetadata) -> Result<(), LlbError> {
    writeln!(w, "  metadata:")?;
    if md.ignore_cache {
        writeln!(w, "    ignore_cache: true")?;
    }
    if !md.description.is_empty() {
        writeln!(w, "    description: {:?}", md.description)?;
    }
    if let Some(export_cache) = &md.export_cache {
        writeln!(w, "    export_cache: {}", export_cache.value)?;
    }
    if !md.caps.is_empty() {
        let mut caps: Vec<&String> = md.caps.keys().collect();
        caps.sort();
        writeln!(w, "    caps: {:?}", caps)?;
    }
    if let Some(pg) = &md.progress_group {
        writeln!(
            w,
            "    progress_group: id={}, name={}, weak={}",
            pg.id, pg.name, pg.weak
        )?;
    }
    Ok(())
}

fn mount_type_name(value: i32) -> &'static str {
    pb::MountType::try_from(value)
        .map(|t| t.as_str_name())
        .unwrap_or("UNKNOWN")
}

fn net_mode_name(value: i32) -> &'static str {
    pb::NetMode::try_from(value)
        .map(|t| t.as_str_name())
        .unwrap_or("UNKNOWN")
}

fn security_mode_name(value: i32) -> &'static str {
    pb::SecurityMode::try_from(value)
        .map(|t| t.as_str_name())
        .unwrap_or("UNKNOWN")
}

#[cfg(feature = "dump_json")]
mod json {
    use std::io::{self, Write};

    use bollard_buildkit_proto::pb;
    use prost::Message;
    use serde_json::{json, Value};

    use crate::definition::Definition;
    use crate::error::LlbError;
    use crate::marshal::sha256;

    /// Write a JSON Lines dump of `def` to `w`.
    ///
    /// Each line is a JSON object with `Op`, `Digest`, and `OpMetadata` fields,
    /// matching the shape produced by `buildctl debug dump-llb`.
    pub fn dump_json<W: Write>(def: &Definition, w: &mut W) -> Result<(), LlbError> {
        for bytes in &def.def {
            let op = pb::Op::decode(bytes.as_slice()).map_err(|source| LlbError::Decode {
                op: "dump_json".to_string(),
                source,
            })?;
            let digest = sha256(bytes);
            let md = def.metadata.get(digest.as_str());
            let entry = json!({
                "Op": op_to_value(&op),
                "Digest": digest.as_str(),
                "OpMetadata": md.map(metadata_to_value).unwrap_or(Value::Null),
            });
            writeln!(
                w,
                "{}",
                serde_json::to_string(&entry).map_err(io::Error::other)?
            )?;
        }
        Ok(())
    }

    fn op_to_value(op: &pb::Op) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "inputs".to_string(),
            op.inputs.iter().map(input_to_value).collect(),
        );
        match &op.op {
            Some(pb::op::Op::Exec(exec)) => {
                obj.insert("exec".to_string(), exec_to_value(exec));
            }
            Some(pb::op::Op::Source(source)) => {
                obj.insert("source".to_string(), source_to_value(source));
            }
            Some(pb::op::Op::File(file)) => {
                obj.insert("file".to_string(), file_to_value(file));
            }
            Some(pb::op::Op::Build(build)) => {
                obj.insert("build".to_string(), build_to_value(build));
            }
            Some(pb::op::Op::Merge(merge)) => {
                obj.insert("merge".to_string(), merge_to_value(merge));
            }
            Some(pb::op::Op::Diff(diff)) => {
                obj.insert("diff".to_string(), diff_to_value(diff));
            }
            None => {}
        }
        obj.insert(
            "platform".to_string(),
            op.platform.as_ref().map_or(Value::Null, platform_to_value),
        );
        obj.insert(
            "constraints".to_string(),
            op.constraints
                .as_ref()
                .map_or(Value::Null, constraints_to_value),
        );
        Value::Object(obj)
    }

    fn input_to_value(input: &pb::Input) -> Value {
        json!({
            "digest": input.digest,
            "index": input.index,
        })
    }

    fn platform_to_value(platform: &pb::Platform) -> Value {
        json!({
            "Architecture": platform.architecture,
            "OS": platform.os,
            "Variant": platform.variant,
            "OSVersion": platform.os_version,
            "OSFeatures": platform.os_features,
        })
    }

    fn constraints_to_value(constraints: &pb::WorkerConstraints) -> Value {
        json!({
            "filter": constraints.filter,
        })
    }

    fn exec_to_value(exec: &pb::ExecOp) -> Value {
        json!({
            "meta": exec.meta.as_ref().map(meta_to_value).unwrap_or(Value::Null),
            "mounts": exec.mounts.iter().map(mount_to_value).collect::<Vec<_>>(),
            "network": pb::NetMode::try_from(exec.network).map(|v| v.as_str_name()).unwrap_or("UNKNOWN"),
            "security": pb::SecurityMode::try_from(exec.security).map(|v| v.as_str_name()).unwrap_or("UNKNOWN"),
            "secretenv": exec.secretenv.iter().map(secret_env_to_value).collect::<Vec<_>>(),
        })
    }

    fn meta_to_value(meta: &pb::Meta) -> Value {
        json!({
            "args": meta.args,
            "env": meta.env,
            "cwd": meta.cwd,
            "user": meta.user,
            "hostname": meta.hostname,
            "cgroupParent": meta.cgroup_parent,
            "removeMountStubsRecursive": meta.remove_mount_stubs_recursive,
            "validExitCodes": meta.valid_exit_codes,
        })
    }

    fn mount_to_value(mount: &pb::Mount) -> Value {
        json!({
            "input": mount.input,
            "selector": mount.selector,
            "dest": mount.dest,
            "output": mount.output,
            "readonly": mount.readonly,
            "mountType": pb::MountType::try_from(mount.mount_type).map(|v| v.as_str_name()).unwrap_or("UNKNOWN"),
            "tmpfsOpt": mount.tmpfs_opt.as_ref().map_or(Value::Null, tmpfs_opt_to_value),
            "cacheOpt": mount.cache_opt.as_ref().map_or(Value::Null, cache_opt_to_value),
            "secretOpt": mount.secret_opt.as_ref().map_or(Value::Null, secret_opt_to_value),
            "sshOpt": mount.ssh_opt.as_ref().map_or(Value::Null, ssh_opt_to_value),
            "resultID": mount.result_id,
        })
    }

    fn tmpfs_opt_to_value(opt: &pb::TmpfsOpt) -> Value {
        json!({"size": opt.size})
    }

    fn cache_opt_to_value(opt: &pb::CacheOpt) -> Value {
        json!({
            "id": opt.id,
            "sharing": pb::CacheSharingOpt::try_from(opt.sharing).map(|v| v.as_str_name()).unwrap_or("UNKNOWN"),
        })
    }

    fn secret_opt_to_value(opt: &pb::SecretOpt) -> Value {
        json!({
            "id": opt.id,
            "uid": opt.uid,
            "gid": opt.gid,
            "mode": opt.mode,
            "optional": opt.optional,
        })
    }

    fn ssh_opt_to_value(opt: &pb::SshOpt) -> Value {
        json!({
            "id": opt.id,
            "uid": opt.uid,
            "gid": opt.gid,
            "mode": opt.mode,
            "optional": opt.optional,
        })
    }

    fn secret_env_to_value(secret: &pb::SecretEnv) -> Value {
        json!({
            "id": secret.id,
            "name": secret.name,
            "optional": secret.optional,
        })
    }

    fn source_to_value(source: &pb::SourceOp) -> Value {
        json!({
            "identifier": source.identifier,
            "attrs": source.attrs,
        })
    }

    fn file_to_value(file: &pb::FileOp) -> Value {
        json!({
            "actions": file.actions.iter().map(file_action_to_value).collect::<Vec<_>>(),
        })
    }

    fn file_action_to_value(action: &pb::FileAction) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert("input".to_string(), json!(action.input));
        obj.insert("secondaryInput".to_string(), json!(action.secondary_input));
        obj.insert("output".to_string(), json!(action.output));
        match &action.action {
            Some(pb::file_action::Action::Copy(copy)) => {
                obj.insert("copy".to_string(), json!({
                    "src": copy.src,
                    "dest": copy.dest,
                    "mode": copy.mode,
                    "followSymlink": copy.follow_symlink,
                    "dirCopyContents": copy.dir_copy_contents,
                    "attemptUnpackDockerCompatibility": copy.attempt_unpack_docker_compatibility,
                    "createDestPath": copy.create_dest_path,
                    "allowWildcard": copy.allow_wildcard,
                    "allowEmptyWildcard": copy.allow_empty_wildcard,
                    "timestamp": copy.timestamp,
                    "includePatterns": copy.include_patterns,
                    "excludePatterns": copy.exclude_patterns,
                    "alwaysReplaceExistingDestPaths": copy.always_replace_existing_dest_paths,
                }));
            }
            Some(pb::file_action::Action::Mkfile(mkfile)) => {
                obj.insert(
                    "mkfile".to_string(),
                    json!({
                        "path": mkfile.path,
                        "mode": mkfile.mode,
                        "data": mkfile.data,
                        "timestamp": mkfile.timestamp,
                    }),
                );
            }
            Some(pb::file_action::Action::Mkdir(mkdir)) => {
                obj.insert(
                    "mkdir".to_string(),
                    json!({
                        "path": mkdir.path,
                        "mode": mkdir.mode,
                        "makeParents": mkdir.make_parents,
                        "timestamp": mkdir.timestamp,
                    }),
                );
            }
            Some(pb::file_action::Action::Rm(rm)) => {
                obj.insert(
                    "rm".to_string(),
                    json!({
                        "path": rm.path,
                        "allowNotFound": rm.allow_not_found,
                        "allowWildcard": rm.allow_wildcard,
                    }),
                );
            }
            Some(pb::file_action::Action::Symlink(symlink)) => {
                obj.insert(
                    "symlink".to_string(),
                    json!({
                        "oldpath": symlink.oldpath,
                        "newpath": symlink.newpath,
                        "timestamp": symlink.timestamp,
                    }),
                );
            }
            None => {}
        }
        Value::Object(obj)
    }

    fn build_to_value(build: &pb::BuildOp) -> Value {
        let inputs: serde_json::Map<String, Value> = build
            .inputs
            .iter()
            .map(|(k, v)| (k.clone(), json!({"input": v.input})))
            .collect();
        json!({
            "builder": build.builder,
            "inputs": inputs,
            "attrs": build.attrs,
        })
    }

    fn merge_to_value(merge: &pb::MergeOp) -> Value {
        json!({
            "inputs": merge.inputs.iter().map(|i| json!({"input": i.input})).collect::<Vec<_>>(),
        })
    }

    fn diff_to_value(diff: &pb::DiffOp) -> Value {
        json!({
            "lower": diff.lower.as_ref().map(|l| json!({"input": l.input})).unwrap_or(Value::Null),
            "upper": diff.upper.as_ref().map(|u| json!({"input": u.input})).unwrap_or(Value::Null),
        })
    }

    fn metadata_to_value(md: &pb::OpMetadata) -> Value {
        json!({
            "ignore_cache": md.ignore_cache,
            "description": md.description,
            "export_cache": md.export_cache.as_ref().map(|e| json!({"Value": e.value})).unwrap_or(Value::Null),
            "caps": md.caps,
            "progress_group": md.progress_group.as_ref().map_or(Value::Null, progress_group_to_value),
        })
    }

    fn progress_group_to_value(pg: &pb::ProgressGroup) -> Value {
        json!({
            "id": pg.id,
            "name": pg.name,
            "weak": pg.weak,
        })
    }
}

#[cfg(feature = "dump_json")]
pub use json::dump_json;

#[cfg(test)]
mod tests {
    use bollard_buildkit_proto::pb;
    use prost::Message;

    use super::*;
    use crate::state::MarshalOpts;
    use crate::{copy, image, merge, mkdir, mkfile, scratch, shlex};

    fn sample_def() -> Definition {
        image("alpine:latest")
            .unwrap()
            .run(shlex("echo hello").unwrap())
            .root()
            .unwrap()
            .marshal(MarshalOpts::default())
            .unwrap()
    }

    #[test]
    fn write_to_matches_into_bytes() {
        let def = sample_def();
        let mut writer = Vec::new();
        def.write_to(&mut writer).unwrap();
        assert_eq!(writer, def.into_bytes().unwrap());
    }

    #[test]
    fn write_to_round_trips() {
        let def = sample_def();
        let mut writer = Vec::new();
        def.write_to(&mut writer).unwrap();
        let decoded = pb::Definition::decode(writer.as_slice()).unwrap();
        assert_eq!(decoded.def.len(), def.def.len());
    }

    #[test]
    fn dump_text_renders_vertices() {
        let def = sample_def();
        let mut out = Vec::new();
        dump_text(&def, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("source"));
        assert!(text.contains("exec"));
        assert!(text.contains("wrapper"));
        assert!(text.contains("docker-image://docker.io/library/alpine:latest"));
    }

    #[test]
    fn dump_text_renders_merge_and_file() {
        let def = merge(
            vec![
                image("alpine:latest").unwrap(),
                scratch()
                    .unwrap()
                    .file(
                        mkdir("/tmp", 0o755).with_parents(true),
                        crate::FileOpts::new(),
                    )
                    .unwrap(),
            ],
            crate::MergeOpts::new(),
        )
        .unwrap()
        .marshal(MarshalOpts::default())
        .unwrap();
        let mut out = Vec::new();
        dump_text(&def, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("merge"));
        assert!(text.contains("file"));
        assert!(text.contains("mkdir"));
    }

    #[test]
    fn dump_text_renders_copy() {
        let def = image("alpine:latest")
            .unwrap()
            .file(
                copy(image("busybox:latest").unwrap(), "/src", "/dst").with_create_dest_path(true),
                crate::FileOpts::new(),
            )
            .unwrap()
            .marshal(MarshalOpts::default())
            .unwrap();
        let mut out = Vec::new();
        dump_text(&def, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("file"));
        assert!(text.contains("copy"));
    }

    #[test]
    fn dump_text_renders_mkfile() {
        let def = scratch()
            .unwrap()
            .file(mkfile("/hello", 0o644, b"world"), crate::FileOpts::new())
            .unwrap()
            .marshal(MarshalOpts::default())
            .unwrap();
        let mut out = Vec::new();
        dump_text(&def, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("mkfile"));
        assert!(text.contains("/hello"));
    }

    #[cfg(feature = "dump_json")]
    #[test]
    fn dump_json_produces_valid_lines() {
        let def = sample_def();
        let mut out = Vec::new();
        dump_json(&def, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.trim().lines().collect();
        assert_eq!(lines.len(), def.def.len());
        for line in lines {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(value.get("Op").is_some());
            assert!(value.get("Digest").is_some());
            assert!(value.get("OpMetadata").is_some());
        }
    }

    #[cfg(feature = "dump_json")]
    #[test]
    fn dump_json_matches_def_count() {
        let def = merge(
            vec![
                image("alpine:latest").unwrap(),
                image("busybox:latest").unwrap(),
            ],
            crate::MergeOpts::new(),
        )
        .unwrap()
        .marshal(MarshalOpts::default())
        .unwrap();
        let mut out = Vec::new();
        dump_json(&def, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.trim().lines().collect();
        assert_eq!(lines.len(), 4); // 2 sources + merge + wrapper
    }
}
