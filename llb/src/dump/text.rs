//! Human-readable LLB definition dumps.
//!
//! The binary encoding of a definition is provided by
//! The binary encoding is provided by
//! [`Definition::write_to`](crate::definition::Definition::write_to).

use bollard_buildkit_proto::pb;
use std::io::Write;

use super::{for_each_op, mount_type_name, net_mode_name, security_mode_name};
use crate::definition::Definition;
use crate::error::LlbError;

/// Write a human-readable text dump of `def` to `w`.
///
/// Output is one vertex at a time in the same order as `def.def` (children
/// before parents). No extra dependencies are required for this format.
pub fn dump_text<W: Write>(def: &Definition, w: &mut W) -> Result<(), LlbError> {
    for_each_op(
        def,
        |digest, op, md| {
            dump_text_op(w, digest, op, md)?;
            writeln!(w)?;
            Ok(())
        },
        "dump_text",
    )
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
    fn write_to_matches_encoded_definition() {
        let def = sample_def();
        let mut writer = Vec::new();
        def.write_to(&mut writer).unwrap();
        assert_eq!(writer, def.clone().into_bytes().unwrap());
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
}
