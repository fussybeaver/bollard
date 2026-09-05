//! JSON Lines LLB definition dumps.

use std::io::{self, Write};

use bollard_buildkit_proto::pb;
use serde_json::{json, Value};

use crate::definition::Definition;
use crate::error::LlbError;

use super::{for_each_op, mount_type_name, net_mode_name, security_mode_name};

/// Write a JSON Lines dump of `def` to `w`.
///
/// Each line is a JSON object with `Op`, `Digest`, and `OpMetadata` fields,
/// matching the shape produced by `buildctl debug dump-llb`.
pub fn dump_json<W: Write>(def: &Definition, w: &mut W) -> Result<(), LlbError> {
    for_each_op(
        def,
        |digest, op, md| {
            let entry = json!({
                "Op": op_to_value(op),
                "Digest": digest.as_str(),
                "OpMetadata": md.map(metadata_to_value).unwrap_or(Value::Null),
            });
            writeln!(
                w,
                "{}",
                serde_json::to_string(&entry).map_err(io::Error::other)?
            )?;
            Ok(())
        },
        "dump_json",
    )
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
        "network": net_mode_name(exec.network),
        "security": security_mode_name(exec.security),
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
        "mountType": mount_type_name(mount.mount_type),
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
        "sharing": pb::CacheSharingOpt::try_from(opt.sharing)
            .map(|v| v.as_str_name())
            .unwrap_or("UNKNOWN"),
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
            obj.insert(
                "copy".to_string(),
                json!({
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
                }),
            );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MarshalOpts;
    use crate::{image, merge, shlex};

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
    fn dump_json_produces_valid_lines() {
        let def = sample_def();
        let mut out = Vec::new();
        dump_json(&def, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.trim().lines().collect();
        assert_eq!(lines.len(), def.def.len());
        for line in lines {
            let value: Value = serde_json::from_str(line).unwrap();
            assert!(value.get("Op").is_some());
            assert!(value.get("Digest").is_some());
            assert!(value.get("OpMetadata").is_some());
        }
    }

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
