//! Execution-operation types: mounts, secrets, cache mounts, run options, and
//! [`ExecOp`] serialization.

use bollard_buildkit_proto::pb;
use prost::Message;

use crate::error::LlbError;
use crate::marshal::{sha256_op, Digest};
use crate::metadata::{attr, cap, OpMetadata};
use crate::ops::{Context, Node, NodeRef, Operation, OperationOutput, OutputIdx};
use crate::state::{ExecState, RunOpts, State};

/// How a cache mount is shared between concurrent builds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CacheSharingMode {
    /// Concurrent reads, no write locking.
    Shared,
    /// Serializes writes.
    #[default]
    Locked,
    /// Fully private mount.
    Private,
}

impl CacheSharingMode {
    /// Return the protobuf [`pb::CacheSharingOpt`] discriminant value.
    pub(crate) fn as_i32(&self) -> i32 {
        match self {
            CacheSharingMode::Shared => pb::CacheSharingOpt::Shared as i32,
            CacheSharingMode::Private => pb::CacheSharingOpt::Private as i32,
            CacheSharingMode::Locked => pb::CacheSharingOpt::Locked as i32,
        }
    }
}

/// Network mode for an exec step.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NetMode {
    /// Use a sandboxed network (default for most builds).
    #[default]
    Sandbox,
    /// Use the host network namespace.
    Host,
    /// No network access.
    None,
}

impl NetMode {
    /// Return the protobuf [`pb::NetMode`] discriminant value.
    pub(crate) fn as_i32(&self) -> i32 {
        match self {
            // Go's default "sandbox" is represented by the proto `Unset` value.
            NetMode::Sandbox => pb::NetMode::Unset as i32,
            NetMode::Host => pb::NetMode::Host as i32,
            NetMode::None => pb::NetMode::None as i32,
        }
    }
}

/// Security mode for an exec step.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SecurityMode {
    /// Run in a sandbox (default).
    #[default]
    Sandbox,
    /// Run insecurely, equivalent to `docker run --security=insecure`.
    Insecure,
}

impl SecurityMode {
    /// Return the protobuf [`pb::SecurityMode`] discriminant value.
    pub(crate) fn as_i32(&self) -> i32 {
        match self {
            SecurityMode::Sandbox => pb::SecurityMode::Sandbox as i32,
            SecurityMode::Insecure => pb::SecurityMode::Insecure as i32,
        }
    }
}

/// A mount inside an exec container.
#[derive(Clone, Debug)]
pub struct Mount {
    /// Mount destination path inside the container.
    pub target: String,
    /// Source state, if any (`None` for scratch mounts).
    pub source: Option<State>,
    /// Mount type.
    pub mount_type: MountType,
    /// Whether the mount is read-only.
    pub readonly: bool,
    /// Output index exposed by this mount, if it is an output mount.
    pub output: Option<u32>,
}

/// Mount type for [`Mount`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MountType {
    /// Bind mount from an input state.
    Bind,
    /// Ephemeral scratch mount.
    Scratch,
    /// Persistent cache mount.
    Cache {
        /// Cache namespace ID.
        id: String,
        /// Sharing mode.
        mode: CacheSharingMode,
    },
    /// Secret mount.
    Secret {
        /// Secret ID.
        id: String,
        /// Optional file path when mounted as a file.
        target: Option<String>,
        /// Whether the secret is optional.
        optional: bool,
    },
    /// SSH agent mount.
    Ssh,
}

/// Add a secret to an exec step.
#[derive(Clone, Debug)]
pub struct AddSecret {
    /// ID of the secret.
    pub id: String,
    /// Also expose the secret as an environment variable.
    pub as_env: bool,
    /// Name of the environment variable when `as_env` is true.
    pub env_name: Option<String>,
    /// Optional file mount path when not exposed only as an env var.
    pub target: Option<String>,
    /// Whether the secret is optional.
    pub optional: bool,
}

impl<S: Into<String>> From<S> for AddSecret {
    fn from(id: S) -> Self {
        Self {
            id: id.into(),
            as_env: false,
            env_name: None,
            target: None,
            optional: false,
        }
    }
}

/// Command arguments for an exec step.
#[derive(Clone, Debug)]
pub struct Shlex {
    /// Argument vector.
    pub args: Vec<String>,
}

impl Shlex {
    /// Split a command string into arguments using POSIX shell rules.
    ///
    /// If the input cannot be parsed (e.g. an unclosed quote), the whole
    /// string is returned as a single argument.
    pub fn new<S: Into<String>>(cmd: S) -> Self {
        let cmd = cmd.into();
        let args = shlex::split(&cmd).unwrap_or_else(|| vec![cmd]);
        Self { args }
    }

    /// Build a [`Shlex`] from an explicit argument list.
    pub fn from_args<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

/// Create a [`Shlex`] from a shell command string.
pub fn shlex<S: Into<String>>(cmd: S) -> Shlex {
    Shlex::new(cmd)
}

/// Add a mount to an exec step.
#[derive(Clone, Debug)]
pub struct AddMount {
    /// Destination path inside the container.
    pub target: String,
    /// Source state to mount.
    pub source: State,
    /// Mount options.
    pub mount_type: MountType,
}

/// Add an environment variable to an exec step.
#[derive(Clone, Debug)]
pub struct AddEnv {
    /// Environment variable name.
    pub key: String,
    /// Environment variable value.
    pub value: String,
}

/// Set a custom name (description) on an operation.
#[derive(Clone, Debug)]
pub struct WithCustomName {
    /// Human-readable name.
    pub name: String,
}

/// A fully assembled execution operation.
#[derive(Clone, Debug)]
pub(crate) struct ExecOp {
    inputs: Vec<OperationOutput>,
    bytes: Vec<u8>,
    digest: Digest,
    metadata: OpMetadata,
}

impl ExecOp {
    /// Build a new exec operation from the base state, inherited constraints,
    /// and run-time options.
    pub(crate) fn new(
        base: OperationOutput,
        cwd: Option<String>,
        env: Vec<(String, String)>,
        mut run: RunOpts,
    ) -> Self {
        // Sort mounts by target path to match Go's ExecOp.Marshal ordering.
        // The rootfs mount at "/" is added separately and remains first.
        run.mounts.sort_by(|a, b| a.target.cmp(&b.target));

        // Collect operation inputs. The base state is always input 0 (the
        // rootfs). Additional inputs are deduplicated by content digest so that
        // two mounts referencing the same source share an input index.
        let mut inputs: Vec<OperationOutput> = vec![base.clone()];
        let mut input_keys: Vec<(Digest, OutputIdx)> =
            vec![(base.operation().digest().clone(), base.index())];
        let mut mount_input_indices: Vec<i64> = Vec::with_capacity(run.mounts.len());

        for mount in &run.mounts {
            if let Some(source) = &mount.source {
                let digest = source.output().operation().digest().clone();
                let index = source.output().index();
                if let Some(pos) = input_keys
                    .iter()
                    .position(|(d, i)| d == &digest && i == &index)
                {
                    mount_input_indices.push(pos as i64);
                } else {
                    let pos = inputs.len() as i64;
                    inputs.push(source.output().clone());
                    input_keys.push((digest, index));
                    mount_input_indices.push(pos);
                }
            } else {
                mount_input_indices.push(-1);
            }
        }

        let pb_inputs: Vec<pb::Input> = input_keys
            .into_iter()
            .map(|(digest, index)| pb::Input {
                digest: digest.as_str().to_string(),
                index: index.0 as i64,
            })
            .collect();

        // The rootfs is always the first mount and produces the primary output.
        let mut pb_mounts: Vec<pb::Mount> = vec![pb::Mount {
            input: 0,
            selector: String::new(),
            dest: "/".to_string(),
            output: 0,
            readonly: false,
            mount_type: pb::MountType::Bind as i32,
            tmpfs_opt: None,
            cache_opt: None,
            secret_opt: None,
            ssh_opt: None,
            result_id: String::new(),
            content_cache: 0,
        }];

        for (mount, input) in run.mounts.iter().zip(mount_input_indices) {
            pb_mounts.push(build_pb_mount(mount, input));
        }

        let merged_env = merge_env(&env, &run.env);
        let meta = pb::Meta {
            args: run.args.clone(),
            env: merged_env
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect(),
            cwd: cwd.unwrap_or_else(|| "/".to_string()),
            user: String::new(),
            proxy_env: None,
            extra_hosts: Vec::new(),
            hostname: String::new(),
            ulimit: Vec::new(),
            cgroup_parent: String::new(),
            remove_mount_stubs_recursive: false,
            valid_exit_codes: Vec::new(),
        };

        let secretenv: Vec<pb::SecretEnv> = run
            .secrets
            .iter()
            .filter(|s| s.as_env)
            .map(|s| pb::SecretEnv {
                id: s.id.clone(),
                name: s.env_name.clone().unwrap_or_else(|| s.id.clone()),
                optional: s.optional,
            })
            .collect();

        let exec = pb::ExecOp {
            meta: Some(meta),
            mounts: pb_mounts,
            network: run.net.as_i32(),
            security: run.security.as_i32(),
            secretenv,
            cdi_devices: Vec::new(),
        };

        let pb_op = pb::Op {
            inputs: pb_inputs,
            platform: None,
            constraints: None,
            op: Some(pb::op::Op::Exec(exec)),
        };

        let digest = sha256_op(&pb_op).expect("ExecOp protobuf encoding is infallible");
        let mut bytes = Vec::new();
        pb_op
            .encode(&mut bytes)
            .expect("ExecOp protobuf encoding is infallible");

        let metadata = build_exec_metadata(&run);

        Self {
            inputs,
            bytes,
            digest,
            metadata,
        }
    }
}

impl Operation for ExecOp {
    fn digest(&self) -> &Digest {
        &self.digest
    }

    fn serialize(&self, ctx: &mut Context) -> Result<NodeRef, LlbError> {
        for input in &self.inputs {
            ctx.register(input)?;
        }
        Ok(ctx.insert_node(Node {
            bytes: self.bytes.clone(),
            digest: self.digest.clone(),
            metadata: self.metadata.clone(),
        }))
    }
}

fn build_pb_mount(mount: &Mount, input: i64) -> pb::Mount {
    let (mount_type, cache_opt, secret_opt, ssh_opt) = match &mount.mount_type {
        MountType::Bind | MountType::Scratch => (pb::MountType::Bind as i32, None, None, None),
        MountType::Cache { id, mode } => (
            pb::MountType::Cache as i32,
            Some(pb::CacheOpt {
                id: id.clone(),
                sharing: mode.as_i32(),
            }),
            None,
            None,
        ),
        MountType::Secret { id, optional, .. } => (
            pb::MountType::Secret as i32,
            None,
            Some(pb::SecretOpt {
                id: id.clone(),
                uid: 0,
                gid: 0,
                mode: 0o444,
                optional: *optional,
            }),
            None,
        ),
        MountType::Ssh => (
            pb::MountType::Ssh as i32,
            None,
            None,
            Some(pb::SshOpt {
                id: String::new(),
                uid: 0,
                gid: 0,
                mode: 0,
                optional: false,
            }),
        ),
    };

    pb::Mount {
        input,
        selector: String::new(),
        dest: mount.target.clone(),
        output: mount.output.map(|o| o as i64).unwrap_or(-1),
        readonly: mount.readonly,
        mount_type,
        tmpfs_opt: None,
        cache_opt,
        secret_opt,
        ssh_opt,
        result_id: String::new(),
        content_cache: 0,
    }
}

fn merge_env(base: &[(String, String)], run: &[(String, String)]) -> Vec<(String, String)> {
    let mut merged = base.to_vec();
    for (key, value) in run {
        if let Some(pos) = merged.iter().position(|(k, _)| k == key) {
            merged[pos].1 = value.clone();
        } else {
            merged.push((key.clone(), value.clone()));
        }
    }
    merged
}

fn build_exec_metadata(run: &RunOpts) -> OpMetadata {
    let mut metadata = OpMetadata::default();
    metadata.caps.insert(cap::CAP_EXEC_META_BASE.to_string());

    if run.net != NetMode::Sandbox {
        metadata.caps.insert(cap::CAP_EXEC_META_NETWORK.to_string());
    }
    if run.security != SecurityMode::Sandbox {
        metadata
            .caps
            .insert(cap::CAP_EXEC_META_SECURITY.to_string());
    }

    for mount in &run.mounts {
        match &mount.mount_type {
            MountType::Bind | MountType::Scratch => {
                metadata.caps.insert(cap::CAP_EXEC_MOUNT_BIND.to_string());
            }
            MountType::Cache { .. } => {
                metadata.caps.insert(cap::CAP_EXEC_MOUNT_CACHE.to_string());
                metadata
                    .caps
                    .insert(cap::CAP_EXEC_MOUNT_CACHE_SHARING.to_string());
            }
            MountType::Secret { .. } => {
                metadata.caps.insert(cap::CAP_EXEC_MOUNT_SECRET.to_string());
            }
            MountType::Ssh => {
                metadata.caps.insert(cap::CAP_EXEC_MOUNT_SSH.to_string());
            }
        }
    }

    if run.secrets.iter().any(|s| s.as_env) {
        metadata.caps.insert(cap::CAP_EXEC_SECRET_ENV.to_string());
    }

    if run.ignore_cache {
        metadata.ignore_cache = true;
        metadata.caps.insert(cap::CAP_META_IGNORE_CACHE.to_string());
    }

    if let Some(name) = &run.custom_name {
        metadata
            .description
            .insert(attr::DESCRIPTION_NAME.to_string(), name.clone());
        metadata.caps.insert(cap::CAP_META_DESCRIPTION.to_string());
    }

    metadata
}

impl crate::state::RunOpt for Shlex {
    fn apply(self, exec: &mut ExecState) {
        exec.run.args = self.args;
    }
}

impl crate::state::RunOpt for AddMount {
    fn apply(self, exec: &mut ExecState) {
        exec.run.mounts.push(Mount {
            target: self.target,
            source: Some(self.source),
            mount_type: self.mount_type,
            readonly: false,
            output: None,
        });
    }
}

impl crate::state::RunOpt for AddSecret {
    fn apply(self, exec: &mut ExecState) {
        exec.run.secrets.push(self);
    }
}

impl crate::state::RunOpt for AddEnv {
    fn apply(self, exec: &mut ExecState) {
        exec.run.env.push((self.key, self.value));
    }
}

impl crate::state::RunOpt for WithCustomName {
    fn apply(self, exec: &mut ExecState) {
        exec.run.custom_name = Some(self.name);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::ops::source::Scratch;
    use crate::ops::OperationOutput;
    use crate::scratch;

    #[test]
    fn execop_digest_stable() {
        let base = OperationOutput::Owned(Arc::new(Scratch::new()));
        let a = ExecOp::new(
            base.clone(),
            None,
            Vec::new(),
            RunOpts::default().with_arg("echo"),
        );
        let b = ExecOp::new(base, None, Vec::new(), RunOpts::default().with_arg("echo"));
        assert_eq!(a.digest, b.digest);
    }

    #[test]
    fn execop_digest_differs_by_args() {
        let base = OperationOutput::Owned(Arc::new(Scratch::new()));
        let a = ExecOp::new(
            base.clone(),
            None,
            Vec::new(),
            RunOpts::default().with_arg("echo"),
        );
        let b = ExecOp::new(base, None, Vec::new(), RunOpts::default().with_arg("cat"));
        assert_ne!(a.digest, b.digest);
    }

    #[test]
    fn execop_mount_input_dedup() {
        // Use an image for the base so the base digest differs from the mount source.
        let base =
            OperationOutput::Owned(Arc::new(crate::ops::source::Image::new("alpine:latest")));
        let src = scratch();
        let run = RunOpts::default()
            .with_arg("echo")
            .with_mount("/a", src.clone())
            .with_mount("/b", src);
        let op = ExecOp::new(base, None, Vec::new(), run);
        // base + one deduplicated mount source = 2 inputs
        assert_eq!(op.inputs.len(), 2);
    }

    #[test]
    fn shlex_splits_command() {
        let s = shlex("echo hello world");
        assert_eq!(s.args, vec!["echo", "hello", "world"]);
    }

    #[test]
    fn shlex_from_args() {
        let s = Shlex::from_args(["echo", "hello"]);
        assert_eq!(s.args, vec!["echo", "hello"]);
    }

    #[test]
    fn exec_state_root_chains() {
        let s = scratch().run(shlex("echo hello")).root();
        let _ = s.run(shlex("echo again")).root();
    }

    #[test]
    fn execop_rootfs_mount() {
        let base = OperationOutput::Owned(Arc::new(Scratch::new()));
        let op = ExecOp::new(base, None, Vec::new(), RunOpts::default().with_arg("echo"));
        let pb_op = pb::Op::decode(op.bytes.as_slice()).unwrap();
        let exec = match pb_op.op {
            Some(pb::op::Op::Exec(e)) => e,
            _ => panic!("expected ExecOp"),
        };
        assert_eq!(exec.mounts[0].dest, "/");
        assert_eq!(exec.mounts[0].input, 0);
    }

    #[test]
    fn execop_env_merge_run_overrides_base() {
        let base = OperationOutput::Owned(Arc::new(Scratch::new()));
        let run = RunOpts::default().with_env("K", "V2");
        let op = ExecOp::new(base, None, vec![("K".to_string(), "V1".to_string())], run);
        let pb_op = pb::Op::decode(op.bytes.as_slice()).unwrap();
        let exec = match pb_op.op {
            Some(pb::op::Op::Exec(e)) => e,
            _ => panic!("expected ExecOp"),
        };
        let meta = exec.meta.expect("expected Meta");
        assert!(meta.env.contains(&"K=V2".to_string()));
        assert!(!meta.env.contains(&"K=V1".to_string()));
    }
}
