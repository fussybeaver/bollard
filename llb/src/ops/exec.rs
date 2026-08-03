//! Execution-operation types: mounts, secrets, cache mounts, run options, and
//! [`ExecOp`] serialization.

use crate::error::LlbError;
use crate::marshal::{encode_and_hash, Digest};
use crate::metadata::{attr, cap, OpMetadata};
use crate::ops::{Context, Node, NodeRef, Operation, OperationOutput, OutputIdx};
use crate::platform::Platform;
use crate::state::{ExecState, RunOpts, State};
use bollard_buildkit_proto::pb;

/// How a cache mount is shared between concurrent builds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CacheSharingMode {
    /// Concurrent reads, no write locking (default; matches Go's
    /// `AsPersistentCacheDir` default).
    #[default]
    Shared,
    /// Serializes writes.
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
        /// UID for a file-mounted secret.
        uid: u32,
        /// GID for a file-mounted secret.
        gid: u32,
        /// File mode for a file-mounted secret.
        mode: u32,
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
    /// UID for a file-mounted secret.
    pub uid: u32,
    /// GID for a file-mounted secret.
    pub gid: u32,
    /// File mode for a file-mounted secret.
    pub mode: u32,
}

impl Default for AddSecret {
    fn default() -> Self {
        Self {
            id: String::new(),
            as_env: false,
            env_name: None,
            target: None,
            optional: false,
            uid: 0,
            gid: 0,
            mode: 0o400,
        }
    }
}

impl<S: Into<String>> From<S> for AddSecret {
    fn from(id: S) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
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
    base: OperationOutput,
    platform: Option<Platform>,
    cwd: Option<String>,
    env: Vec<(String, String)>,
    run: RunOpts,
    metadata: OpMetadata,
}

impl ExecOp {
    /// Build a new exec operation from the base state, inherited constraints,
    /// and run-time options.
    ///
    /// The actual protobuf bytes are computed at marshal time so that the
    /// active platform and worker constraints affect the content digest.
    pub(crate) fn new(
        base: OperationOutput,
        platform: Option<Platform>,
        cwd: Option<String>,
        env: Vec<(String, String)>,
        mut run: RunOpts,
    ) -> Result<Self, LlbError> {
        // Sort mounts by target path to match Go's moby/buildkit client/llb
        // ExecOp.Marshal behavior (github.com/moby/buildkit@v0.31.1,
        // client/llb/exec.go:145-148). This canonicalization keeps cache keys
        // stable regardless of the order in which mounts were added.
        // The rootfs mount at "/" is added separately and remains first.
        run.mounts.sort_by(|a, b| a.target.cmp(&b.target));

        let metadata = build_exec_metadata(&run, !base.is_empty());
        Ok(Self {
            base,
            platform,
            cwd,
            env,
            run,
            metadata,
        })
    }
}

impl Operation for ExecOp {
    fn serialize(&self, ctx: &mut Context) -> Result<NodeRef, LlbError> {
        // Collect operation inputs. The base state is input 0 when it is a real
        // operation; an empty (scratch) base is encoded as input index -1.
        // Additional inputs are deduplicated by content digest so that two
        // mounts referencing the same source share an input index.
        let mut inputs: Vec<OperationOutput> = Vec::new();
        let mut input_keys: Vec<(Digest, OutputIdx)> = Vec::new();
        let mut mount_input_indices: Vec<i64> = Vec::with_capacity(self.run.mounts.len() + 1);

        if self.base.is_empty() {
            mount_input_indices.push(-1);
        } else {
            let node_ref = ctx.register(&self.base)?;
            inputs.push(self.base.clone());
            input_keys.push((node_ref.digest().clone(), node_ref.index()));
            mount_input_indices.push(0);
        }

        for mount in &self.run.mounts {
            if let Some(source) = &mount.source {
                if source.output().is_empty() {
                    mount_input_indices.push(-1);
                } else {
                    let output = source.output().clone();
                    let node_ref = ctx.register(&output)?;
                    let key = (node_ref.digest().clone(), node_ref.index());
                    if let Some(pos) = input_keys.iter().position(|k| *k == key) {
                        mount_input_indices.push(pos as i64);
                    } else {
                        let pos = inputs.len() as i64;
                        inputs.push(output);
                        input_keys.push(key);
                        mount_input_indices.push(pos);
                    }
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
            input: mount_input_indices[0],
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

        let mut next_output = 1_i64;
        for (mount, input) in self.run.mounts.iter().zip(&mount_input_indices[1..]) {
            let mut pb_mount = build_pb_mount(mount, *input);
            if pb_mount.output < 0
                && !mount.readonly
                && matches!(mount.mount_type, MountType::Bind | MountType::Scratch)
            {
                pb_mount.output = next_output;
                next_output += 1;
            }
            pb_mounts.push(pb_mount);
        }

        // File secrets are separate mounts in Go's ExecOp. The default wire
        // destination is the secret ID; environment-only secrets have no file
        // mount unless an explicit target is supplied.
        for secret in &self.run.secrets {
            if secret.as_env && secret.target.is_none() {
                continue;
            }
            let target = secret.target.clone().unwrap_or_else(|| secret.id.clone());
            pb_mounts.push(build_pb_mount(
                &Mount {
                    target,
                    source: None,
                    mount_type: MountType::Secret {
                        id: secret.id.clone(),
                        target: secret.target.clone(),
                        optional: secret.optional,
                        uid: secret.uid,
                        gid: secret.gid,
                        mode: secret.mode,
                    },
                    readonly: false,
                    output: Some(0),
                },
                -1,
            ));
        }

        let merged_env = merge_env(&self.env, &self.run.env);
        let meta = pb::Meta {
            args: self.run.args.clone(),
            env: merged_env
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect(),
            cwd: self.cwd.clone().unwrap_or_else(|| "/".to_string()),
            user: String::new(),
            proxy_env: None,
            extra_hosts: Vec::new(),
            hostname: String::new(),
            ulimit: Vec::new(),
            cgroup_parent: String::new(),
            remove_mount_stubs_recursive: true,
            valid_exit_codes: Vec::new(),
        };

        let secretenv: Vec<pb::SecretEnv> = self
            .run
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
            network: self.run.net.as_i32(),
            security: self.run.security.as_i32(),
            secretenv,
            cdi_devices: Vec::new(),
        };

        let pb_op = pb::Op {
            inputs: pb_inputs,
            platform: ctx.combined_platform(self.platform.clone()).map(Into::into),
            constraints: Some(pb::WorkerConstraints {
                filter: ctx.worker_filters().to_vec(),
            }),
            op: Some(pb::op::Op::Exec(exec)),
        };

        let (digest, bytes) = encode_and_hash(&pb_op)?;
        Ok(ctx.insert_node(Node {
            bytes,
            digest,
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
        MountType::Secret {
            id,
            optional,
            uid,
            gid,
            mode,
            ..
        } => (
            pb::MountType::Secret as i32,
            None,
            Some(pb::SecretOpt {
                id: id.clone(),
                uid: *uid,
                gid: *gid,
                mode: *mode,
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

fn build_exec_metadata(run: &RunOpts, root_has_input: bool) -> OpMetadata {
    let mut metadata = OpMetadata::default();
    metadata.caps.insert(cap::CAP_EXEC_META_BASE.to_string());

    if root_has_input {
        metadata.caps.insert(cap::CAP_EXEC_MOUNT_BIND.to_string());
    }

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

    if !run.secrets.is_empty() {
        metadata.caps.insert(cap::CAP_EXEC_MOUNT_SECRET.to_string());
    }

    if run.secrets.iter().any(|s| s.as_env) {
        metadata.caps.insert(cap::CAP_EXEC_SECRET_ENV.to_string());
    }

    if run.ignore_cache {
        metadata.ignore_cache = true;
    }

    if let Some(name) = &run.custom_name {
        metadata
            .description
            .insert(attr::DESCRIPTION_NAME.to_string(), name.clone());
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
