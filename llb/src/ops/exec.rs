//! Execution-operation types: mounts, secrets, cache mounts, run options, and
//! [`ExecOp`] serialization.

use crate::error::LlbError;
use crate::metadata::{attr, cap, OpMetadata};
use crate::ops::{Context, InputPlan, Operation, OperationOutput, SerializedOp};
use crate::platform::Platform;
use crate::state::{ExecState, RunOpts, State};
use bollard_buildkit_proto::pb;
use indexmap::IndexMap;

/// How a cache mount is shared between concurrent builds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
    pub(crate) target: String,
    /// Source state, if any (`None` for scratch mounts).
    pub(crate) source: Option<State>,
    /// Mount type.
    pub(crate) mount_type: MountType,
    /// Whether the mount is read-only.
    pub(crate) readonly: bool,
    /// Output index exposed by this mount, if it is an output mount.
    pub(crate) output: Option<u32>,
}

impl Mount {
    /// Return the mount destination.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Return the source state, if this mount has one.
    pub fn source(&self) -> Option<&State> {
        self.source.as_ref()
    }

    /// Return the mount type.
    pub fn mount_type(&self) -> &MountType {
        &self.mount_type
    }

    /// Return whether the mount is read-only.
    pub fn readonly(&self) -> bool {
        self.readonly
    }

    /// Return the output index, if this mount exposes one.
    pub fn output(&self) -> Option<u32> {
        self.output
    }
}

/// Mount type for [`Mount`].
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
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
}

/// Add a secret to an exec step.
#[derive(Clone, Debug)]
pub struct AddSecret {
    /// ID of the secret.
    pub(crate) id: String,
    /// Also expose the secret as an environment variable.
    pub(crate) as_env: bool,
    /// Name of the environment variable when `as_env` is true.
    pub(crate) env_name: Option<String>,
    /// Optional file mount path when not exposed only as an env var.
    pub(crate) target: Option<String>,
    /// Whether the secret is optional.
    pub(crate) optional: bool,
    /// UID for a file-mounted secret.
    pub(crate) uid: u32,
    /// GID for a file-mounted secret.
    pub(crate) gid: u32,
    /// File mode for a file-mounted secret.
    pub(crate) mode: u32,
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

impl AddSecret {
    /// Create secret options for the given secret ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }

    /// Expose the secret as an environment variable.
    pub fn with_as_env(mut self, as_env: bool) -> Self {
        self.as_env = as_env;
        self
    }

    /// Set the environment variable name.
    pub fn with_env_name(mut self, name: impl Into<String>) -> Self {
        self.env_name = Some(name.into());
        self
    }

    /// Set the file mount target.
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// Set whether the secret may be unavailable.
    pub fn with_optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// Set the file mount UID.
    pub fn with_uid(mut self, uid: u32) -> Self {
        self.uid = uid;
        self
    }

    /// Set the file mount GID.
    pub fn with_gid(mut self, gid: u32) -> Self {
        self.gid = gid;
        self
    }

    /// Set the file mount mode.
    pub fn with_mode(mut self, mode: u32) -> Self {
        self.mode = mode;
        self
    }

    /// Return the secret ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Return whether the secret is exposed as an environment variable.
    pub fn as_env(&self) -> bool {
        self.as_env
    }

    /// Return the configured environment variable name.
    pub fn env_name(&self) -> Option<&str> {
        self.env_name.as_deref()
    }

    /// Return the configured file mount target.
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// Return whether the secret is optional.
    pub fn optional(&self) -> bool {
        self.optional
    }

    /// Return the file mount UID.
    pub fn uid(&self) -> u32 {
        self.uid
    }

    /// Return the file mount GID.
    pub fn gid(&self) -> u32 {
        self.gid
    }

    /// Return the file mount mode.
    pub fn mode(&self) -> u32 {
        self.mode
    }
}

/// Add an SSH agent socket to an exec step.
///
/// This mirrors BuildKit's `llb.AddSSHSocket` option. An empty ID is resolved
/// to BuildKit's `default` provider by the session layer.
#[derive(Clone, Debug)]
pub struct AddSshSocket {
    /// BuildKit SSH provider ID. Empty selects the default provider.
    pub(crate) id: String,
    /// Socket target. An absent or empty target receives BuildKit's default.
    pub(crate) target: Option<String>,
    /// UID for the mounted socket.
    pub(crate) uid: u32,
    /// GID for the mounted socket.
    pub(crate) gid: u32,
    /// File mode for the mounted socket.
    pub(crate) mode: u32,
    /// Whether an unavailable provider is allowed.
    pub(crate) optional: bool,
}

impl Default for AddSshSocket {
    fn default() -> Self {
        Self::new()
    }
}

impl AddSshSocket {
    /// Create an SSH socket option with BuildKit-compatible defaults.
    pub fn new() -> Self {
        Self {
            id: String::new(),
            target: None,
            uid: 0,
            gid: 0,
            mode: 0o600,
            optional: false,
        }
    }

    /// Set the BuildKit SSH provider ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Set the socket target path.
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// Set the socket UID.
    pub fn with_uid(mut self, uid: u32) -> Self {
        self.uid = uid;
        self
    }

    /// Set the socket GID.
    pub fn with_gid(mut self, gid: u32) -> Self {
        self.gid = gid;
        self
    }

    /// Set the socket file mode.
    pub fn with_mode(mut self, mode: u32) -> Self {
        self.mode = mode;
        self
    }

    /// Set whether the provider may be unavailable.
    pub fn with_optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// Return the configured provider ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Return the configured target, if one was supplied.
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// Return the configured UID.
    pub fn uid(&self) -> u32 {
        self.uid
    }

    /// Return the configured GID.
    pub fn gid(&self) -> u32 {
        self.gid
    }

    /// Return the configured file mode.
    pub fn mode(&self) -> u32 {
        self.mode
    }

    /// Return whether the provider is optional.
    pub fn optional(&self) -> bool {
        self.optional
    }
}

impl<S: Into<String>> From<S> for AddSshSocket {
    fn from(id: S) -> Self {
        Self::new().with_id(id)
    }
}

/// Command arguments for an exec step.
#[derive(Clone, Debug)]
pub struct Shlex {
    /// Argument vector.
    pub(crate) args: Vec<String>,
}

impl Shlex {
    /// Split a command string into arguments using POSIX shell rules.
    pub fn new<S: Into<String>>(cmd: S) -> Result<Self, LlbError> {
        let cmd = cmd.into();
        validate_shell(&cmd)?;
        let args = shlex::split(&cmd).ok_or(LlbError::InvalidShell {
            position: cmd.len(),
            kind: "invalid shell syntax",
        })?;
        Ok(Self { args })
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

    /// Return the parsed argument vector.
    pub fn args(&self) -> &[String] {
        &self.args
    }
}

/// Create a [`Shlex`] from a shell command string.
pub fn shlex<S: Into<String>>(cmd: S) -> Result<Shlex, LlbError> {
    Shlex::new(cmd)
}

fn validate_shell(command: &str) -> Result<(), LlbError> {
    let bytes = command.as_bytes();
    let mut quote = None;
    let mut opening = 0;
    let mut escaped = false;

    for (position, byte) in bytes.iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }

        match quote {
            Some(b'\'') => {
                if byte == b'\'' {
                    quote = None;
                }
            }
            Some(b'"') => match byte {
                b'\\' => escaped = true,
                b'"' => quote = None,
                _ => {}
            },
            None => match byte {
                b'\\' => escaped = true,
                b'\'' | b'"' => {
                    quote = Some(byte);
                    opening = position;
                }
                _ => {}
            },
            _ => unreachable!(),
        }
    }

    if escaped {
        return Err(LlbError::InvalidShell {
            position: bytes.len().saturating_sub(1),
            kind: "trailing escape",
        });
    }
    if let Some(delimiter) = quote {
        return Err(LlbError::InvalidShell {
            position: opening,
            kind: if delimiter == b'\'' {
                "unclosed single quote"
            } else {
                "unclosed double quote"
            },
        });
    }

    Ok(())
}

/// Add a mount to an exec step.
#[derive(Clone, Debug)]
pub struct AddMount {
    /// Destination path inside the container.
    pub(crate) target: String,
    /// Source state to mount.
    pub(crate) source: State,
    /// Mount options.
    pub(crate) mount_type: MountType,
}

impl AddMount {
    /// Create a bind mount option.
    pub fn new(target: impl Into<String>, source: State) -> Self {
        Self {
            target: target.into(),
            source,
            mount_type: MountType::Bind,
        }
    }

    /// Set the mount type.
    pub fn with_mount_type(mut self, mount_type: MountType) -> Self {
        self.mount_type = mount_type;
        self
    }

    /// Return the mount target.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Return the source state.
    pub fn source(&self) -> &State {
        &self.source
    }

    /// Return the mount type.
    pub fn mount_type(&self) -> &MountType {
        &self.mount_type
    }
}

/// Add an environment variable to an exec step.
#[derive(Clone, Debug)]
pub struct AddEnv {
    /// Environment variable name.
    pub(crate) key: String,
    /// Environment variable value.
    pub(crate) value: String,
}

impl AddEnv {
    /// Create an environment-variable option.
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }

    /// Return the environment variable name.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Return the environment variable value.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Set a custom name (description) on an operation.
#[derive(Clone, Debug)]
pub struct WithCustomName {
    /// Human-readable name.
    pub(crate) name: String,
}

impl WithCustomName {
    /// Create a custom-name option.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Return the custom name.
    pub fn name(&self) -> &str {
        &self.name
    }
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
        if run.args.is_empty() {
            return Err(LlbError::InvalidExec {
                reason: "arguments are required",
            });
        }

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
    fn build_serialized(&self, ctx: &mut Context) -> Result<SerializedOp, LlbError> {
        // Collect operation inputs. The base state is input 0 when it is a real
        // operation; an empty (scratch) base is encoded as input index -1.
        // Additional inputs are deduplicated by content digest so that two
        // mounts referencing the same source share an input index.
        let input_plan = ExecInputPlan::new(&self.base, &self.run.mounts, ctx)?;
        let pb_inputs = input_plan.pb_inputs();

        // The rootfs is always the first mount and produces the primary output.
        let mut pb_mounts: Vec<pb::Mount> = vec![pb::Mount {
            input: input_plan.mount_input_indices[0],
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
        for (mount, input) in self
            .run
            .mounts
            .iter()
            .zip(&input_plan.mount_input_indices[1..])
        {
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

        // SSH sockets are kept separate from ordinary mounts because Go
        // numbers their default targets in declaration order, not sorted
        // target order.
        let ssh_mounts: Vec<(String, &AddSshSocket)> = self
            .run
            .ssh
            .iter()
            .enumerate()
            .map(|(index, socket)| {
                let target = socket
                    .target
                    .as_deref()
                    .filter(|target| !target.is_empty())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("/run/buildkit/ssh_agent.{index}"));
                (target, socket)
            })
            .collect();

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

        for (target, socket) in &ssh_mounts {
            pb_mounts.push(pb::Mount {
                input: -1,
                selector: String::new(),
                dest: target.clone(),
                output: 0,
                readonly: false,
                mount_type: pb::MountType::Ssh as i32,
                tmpfs_opt: None,
                cache_opt: None,
                secret_opt: None,
                ssh_opt: Some(pb::SshOpt {
                    id: socket.id.clone(),
                    uid: socket.uid,
                    gid: socket.gid,
                    mode: socket.mode,
                    optional: socket.optional,
                }),
                result_id: String::new(),
                content_cache: 0,
            });
        }

        let mut merged_env = merge_env(&self.env, &self.run.env);
        if let Some((target, _)) = ssh_mounts.first() {
            if !merged_env.iter().any(|(key, _)| key == "SSH_AUTH_SOCK") {
                merged_env.push((String::from("SSH_AUTH_SOCK"), target.clone()));
            }
        }
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

        Ok(SerializedOp {
            op: pb_op,
            metadata: self.metadata.clone(),
        })
    }
}

#[derive(Debug)]
struct ExecInputPlan {
    inputs: InputPlan,
    mount_input_indices: Vec<i64>,
}

impl ExecInputPlan {
    fn new(base: &OperationOutput, mounts: &[Mount], ctx: &mut Context) -> Result<Self, LlbError> {
        let mut plan = Self {
            inputs: InputPlan::default(),
            mount_input_indices: Vec::with_capacity(mounts.len() + 1),
        };
        let base_input = plan.inputs.register(base, ctx)?;
        plan.mount_input_indices.push(base_input);
        for mount in mounts {
            let input = match &mount.source {
                Some(source) => plan.inputs.register(source.output(), ctx)?,
                None => -1,
            };
            plan.mount_input_indices.push(input);
        }
        Ok(plan)
    }

    fn pb_inputs(&self) -> Vec<pb::Input> {
        self.inputs.pb_inputs()
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
    base.iter()
        .chain(run)
        .fold(
            IndexMap::with_capacity(base.len() + run.len()),
            |mut merged, (key, value)| {
                merged.insert(key.clone(), value.clone());
                merged
            },
        )
        .into_iter()
        .collect()
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
            MountType::Bind => {
                metadata.caps.insert(cap::CAP_EXEC_MOUNT_BIND.to_string());
            }
            MountType::Scratch => {}
            MountType::Cache { .. } => {
                metadata.caps.insert(cap::CAP_EXEC_MOUNT_CACHE.to_string());
                metadata
                    .caps
                    .insert(cap::CAP_EXEC_MOUNT_CACHE_SHARING.to_string());
            }
            MountType::Secret { .. } => {
                metadata.caps.insert(cap::CAP_EXEC_MOUNT_SECRET.to_string());
            }
        }
    }

    if !run.ssh.is_empty() {
        metadata.caps.insert(cap::CAP_EXEC_MOUNT_SSH.to_string());
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

impl crate::state::private::RunOptSealed for Shlex {}

impl crate::state::RunOpt for Shlex {
    fn apply(self, exec: &mut ExecState) {
        exec.run.args = self.args;
    }
}

impl crate::state::private::RunOptSealed for AddMount {}

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

impl crate::state::private::RunOptSealed for AddSecret {}

impl crate::state::RunOpt for AddSecret {
    fn apply(self, exec: &mut ExecState) {
        exec.run.secrets.push(self);
    }
}

impl crate::state::private::RunOptSealed for AddSshSocket {}

impl crate::state::RunOpt for AddSshSocket {
    fn apply(self, exec: &mut ExecState) {
        exec.run.ssh.push(self);
    }
}

impl crate::state::private::RunOptSealed for AddEnv {}

impl crate::state::RunOpt for AddEnv {
    fn apply(self, exec: &mut ExecState) {
        exec.run.env.push((self.key, self.value));
    }
}

impl crate::state::private::RunOptSealed for WithCustomName {}

impl crate::state::RunOpt for WithCustomName {
    fn apply(self, exec: &mut ExecState) {
        exec.run.custom_name = Some(self.name);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use prost::Message;

    use super::*;
    use crate::ops::OperationOutput;
    use crate::scratch;

    #[test]
    fn cache_sharing_mode_default_is_shared() {
        assert_eq!(CacheSharingMode::default(), CacheSharingMode::Shared);
    }

    #[test]
    fn cache_sharing_mode_as_i32_matches_proto() {
        assert_eq!(
            CacheSharingMode::Shared.as_i32(),
            pb::CacheSharingOpt::Shared as i32
        );
        assert_eq!(
            CacheSharingMode::Locked.as_i32(),
            pb::CacheSharingOpt::Locked as i32
        );
        assert_eq!(
            CacheSharingMode::Private.as_i32(),
            pb::CacheSharingOpt::Private as i32
        );
    }

    fn serialize_exec_op(op: ExecOp) -> (pb::ExecOp, crate::ops::Context) {
        let mut ctx = crate::ops::Context::new(None, Vec::new());
        let node_ref = op.serialize(&mut ctx).unwrap();
        let node = ctx.nodes().get(node_ref.digest()).unwrap();
        let pb_op = pb::Op::decode(node.bytes.as_slice()).unwrap();
        let exec = match pb_op.op {
            Some(pb::op::Op::Exec(exec)) => exec,
            _ => panic!("expected ExecOp"),
        };
        (exec, ctx)
    }

    fn exec_digest(base: OperationOutput, run: RunOpts) -> String {
        let op = ExecOp::new(base, None, None, Vec::new(), run).unwrap();
        let mut ctx = crate::ops::Context::new(None, Vec::new());
        let node_ref = op.serialize(&mut ctx).unwrap();
        node_ref.digest().to_string()
    }

    #[test]
    fn execop_digest_stable() {
        let base = scratch().unwrap().output().clone();
        let a = exec_digest(base.clone(), RunOpts::default().with_arg("echo"));
        let b = exec_digest(base, RunOpts::default().with_arg("echo"));
        assert_eq!(a, b);
    }

    #[test]
    fn execop_digest_differs_by_args() {
        let base = scratch().unwrap().output().clone();
        let a = exec_digest(base.clone(), RunOpts::default().with_arg("echo"));
        let b = exec_digest(base, RunOpts::default().with_arg("cat"));
        assert_ne!(a, b);
    }

    #[test]
    fn exec_rejects_empty_argument_lists() {
        let base = scratch().unwrap().output().clone();
        for run in [RunOpts::default(), RunOpts::from(shlex("").unwrap())] {
            let error = ExecOp::new(base.clone(), None, None, Vec::new(), run).unwrap_err();
            assert!(matches!(
                error,
                LlbError::InvalidExec {
                    reason: "arguments are required"
                }
            ));
        }
    }

    #[test]
    fn exec_default_meta_removes_mount_stubs_recursively() {
        let op = ExecOp::new(
            scratch().unwrap().output().clone(),
            None,
            None,
            Vec::new(),
            RunOpts::default().with_arg("true"),
        )
        .unwrap();
        let (exec, _) = serialize_exec_op(op);
        assert!(
            exec.meta
                .expect("exec metadata should be present")
                .remove_mount_stubs_recursive
        );
    }

    #[test]
    fn execop_mount_input_dedup() {
        let base = OperationOutput::Owned(Arc::new(
            crate::ops::source::Image::new("alpine:latest").unwrap(),
        ));
        let src = crate::image("busybox:latest").unwrap();
        let run = RunOpts::default()
            .with_arg("echo")
            .with_mount("/a", src.clone())
            .with_mount("/b", src);
        let op = ExecOp::new(base, None, None, Vec::new(), run).unwrap();
        let (exec, ctx) = serialize_exec_op(op);
        let node = ctx.nodes().values().last().unwrap();
        let pb_op = pb::Op::decode(node.bytes.as_slice()).unwrap();
        assert_eq!(pb_op.inputs.len(), 2);
        assert_eq!(exec.mounts[1].input, 1);
        assert_eq!(exec.mounts[2].input, 1);
    }

    #[test]
    fn scratch_bind_mount_uses_empty_input_and_output() {
        let state = scratch()
            .unwrap()
            .run(shlex("echo").unwrap())
            .add_mount_scratch("/scratch")
            .root()
            .unwrap();
        let def = state
            .marshal(crate::state::MarshalOpts::linux_amd64())
            .unwrap();
        let exec = def
            .def
            .iter()
            .map(|bytes| pb::Op::decode(bytes.as_slice()).unwrap())
            .find_map(|op| match op.op {
                Some(pb::op::Op::Exec(exec)) => Some(exec),
                _ => None,
            })
            .expect("expected exec operation");

        assert_eq!(exec.mounts[1].input, -1);
        assert_eq!(exec.mounts[1].output, 1);
        let metadata = def
            .metadata
            .values()
            .find(|metadata| metadata.caps.contains_key(cap::CAP_EXEC_META_BASE))
            .expect("expected exec metadata");
        assert!(!metadata.caps.contains_key(cap::CAP_EXEC_MOUNT_BIND));
    }

    #[test]
    fn file_secret_mount_uses_go_defaults() {
        let op = ExecOp::new(
            scratch().unwrap().output().clone(),
            None,
            None,
            Vec::new(),
            RunOpts {
                secrets: vec![AddSecret::from("token")],
                ..RunOpts::default().with_arg("cat")
            },
        )
        .unwrap();
        let (exec, ctx) = serialize_exec_op(op);
        let secret = exec
            .mounts
            .iter()
            .find(|mount| mount.mount_type == pb::MountType::Secret as i32)
            .expect("file secret mount should be emitted");
        let secret_opt = secret.secret_opt.as_ref().expect("secret options");

        assert_eq!(secret.input, -1);
        assert_eq!(secret.output, 0);
        assert_eq!(secret.dest, "token");
        assert_eq!(secret_opt.id, "token");
        assert_eq!(secret_opt.uid, 0);
        assert_eq!(secret_opt.gid, 0);
        assert_eq!(secret_opt.mode, 0o400);
        assert!(!secret_opt.optional);

        let node = ctx.nodes().values().last().expect("exec node");
        assert!(node.metadata.caps.contains(cap::CAP_EXEC_MOUNT_SECRET));
    }

    #[test]
    fn file_secret_mount_preserves_target_permissions_and_optionality() {
        let op = ExecOp::new(
            scratch().unwrap().output().clone(),
            None,
            None,
            Vec::new(),
            RunOpts {
                secrets: vec![AddSecret {
                    target: Some("/etc/license".to_string()),
                    optional: true,
                    uid: 1000,
                    gid: 1001,
                    mode: 0o440,
                    ..AddSecret::from("license")
                }],
                ..RunOpts::default().with_arg("cat")
            },
        )
        .unwrap();
        let (exec, _) = serialize_exec_op(op);
        let secret = exec
            .mounts
            .iter()
            .find(|mount| mount.mount_type == pb::MountType::Secret as i32)
            .expect("file secret mount should be emitted");
        let secret_opt = secret.secret_opt.as_ref().expect("secret options");

        assert_eq!(secret.dest, "/etc/license");
        assert_eq!(secret_opt.uid, 1000);
        assert_eq!(secret_opt.gid, 1001);
        assert_eq!(secret_opt.mode, 0o440);
        assert!(secret_opt.optional);
    }

    #[test]
    fn ssh_mount_uses_go_defaults_and_declaration_order() {
        let op = ExecOp::new(
            scratch().unwrap().output().clone(),
            None,
            None,
            Vec::new(),
            RunOpts::default()
                .with_arg("true")
                .with_mount_scratch("/z")
                .with_mount_scratch("/a")
                .with_ssh_socket(AddSshSocket::new())
                .with_ssh_socket(AddSshSocket::new().with_id("deploy")),
        )
        .unwrap();
        let (exec, ctx) = serialize_exec_op(op);
        let ssh_mounts: Vec<&pb::Mount> = exec
            .mounts
            .iter()
            .filter(|mount| mount.mount_type == pb::MountType::Ssh as i32)
            .collect();

        assert_eq!(ssh_mounts.len(), 2);
        assert_eq!(ssh_mounts[0].dest, "/run/buildkit/ssh_agent.0");
        assert_eq!(ssh_mounts[1].dest, "/run/buildkit/ssh_agent.1");
        assert_eq!(ssh_mounts[0].input, -1);
        assert_eq!(ssh_mounts[0].output, 0);
        assert_eq!(ssh_mounts[0].ssh_opt.as_ref().unwrap().id, "");
        assert_eq!(ssh_mounts[0].ssh_opt.as_ref().unwrap().mode, 0o600);
        assert_eq!(ssh_mounts[1].ssh_opt.as_ref().unwrap().id, "deploy");
        assert!(
            exec.mounts
                .iter()
                .position(|mount| mount.dest == "/a")
                .unwrap()
                < exec
                    .mounts
                    .iter()
                    .position(|mount| mount.dest == "/z")
                    .unwrap()
        );

        let node = ctx.nodes().values().last().unwrap();
        assert!(node.metadata.caps.contains(cap::CAP_EXEC_MOUNT_SSH));
    }

    #[test]
    fn ssh_mount_preserves_explicit_options_and_target() {
        let socket = AddSshSocket::new()
            .with_id("deploy")
            .with_target("/run/deploy.sock")
            .with_uid(1000)
            .with_gid(1001)
            .with_mode(0o640)
            .with_optional(true);
        let op = ExecOp::new(
            scratch().unwrap().output().clone(),
            None,
            None,
            Vec::new(),
            RunOpts::default().with_arg("true").with_ssh_socket(socket),
        )
        .unwrap();
        let (exec, _) = serialize_exec_op(op);
        let ssh = exec
            .mounts
            .iter()
            .find(|mount| mount.mount_type == pb::MountType::Ssh as i32)
            .unwrap();
        let options = ssh.ssh_opt.as_ref().unwrap();

        assert_eq!(ssh.dest, "/run/deploy.sock");
        assert_eq!(options.id, "deploy");
        assert_eq!(options.uid, 1000);
        assert_eq!(options.gid, 1001);
        assert_eq!(options.mode, 0o640);
        assert!(options.optional);
    }

    #[test]
    fn ssh_mount_sets_auth_sock_only_when_missing() {
        let op = ExecOp::new(
            scratch().unwrap().output().clone(),
            None,
            None,
            Vec::new(),
            RunOpts::default()
                .with_arg("true")
                .with_ssh_socket(AddSshSocket::new().with_target("/run/agent.sock")),
        )
        .unwrap();
        let (exec, _) = serialize_exec_op(op);
        let meta = exec.meta.as_ref().unwrap();
        assert!(meta
            .env
            .contains(&String::from("SSH_AUTH_SOCK=/run/agent.sock")));

        let op = ExecOp::new(
            scratch().unwrap().output().clone(),
            None,
            None,
            Vec::new(),
            RunOpts::default()
                .with_arg("true")
                .with_env("SSH_AUTH_SOCK", "/caller/agent.sock")
                .with_ssh_socket(AddSshSocket::new()),
        )
        .unwrap();
        let (exec, _) = serialize_exec_op(op);
        let meta = exec.meta.as_ref().unwrap();
        assert!(meta
            .env
            .contains(&String::from("SSH_AUTH_SOCK=/caller/agent.sock")));
        assert!(!meta
            .env
            .contains(&String::from("SSH_AUTH_SOCK=/run/buildkit/ssh_agent.0")));
    }

    #[test]
    fn environment_only_secret_has_no_file_mount() {
        let op = ExecOp::new(
            scratch().unwrap().output().clone(),
            None,
            None,
            Vec::new(),
            RunOpts {
                secrets: vec![AddSecret {
                    as_env: true,
                    env_name: Some("TOKEN".to_string()),
                    ..AddSecret::from("token")
                }],
                ..RunOpts::default().with_arg("env")
            },
        )
        .unwrap();
        let (exec, _) = serialize_exec_op(op);

        assert_eq!(exec.mounts.len(), 1, "only the root mount is expected");
        assert_eq!(exec.secretenv.len(), 1);
        assert_eq!(exec.secretenv[0].id, "token");
        assert_eq!(exec.secretenv[0].name, "TOKEN");
    }

    #[test]
    fn shlex_splits_command() {
        let s = shlex("echo hello world").unwrap();
        assert_eq!(s.args, vec!["echo", "hello", "world"]);
    }

    #[test]
    fn shlex_from_args() {
        let s = Shlex::from_args(["echo", "hello"]);
        assert_eq!(s.args, vec!["echo", "hello"]);
    }

    #[test]
    fn shlex_rejects_malformed_commands_with_position() {
        for (command, expected_position, expected_kind) in [
            ("echo 'hello", 5, "unclosed single quote"),
            ("echo \\", 5, "trailing escape"),
        ] {
            let Err(crate::error::LlbError::InvalidShell { position, kind }) = shlex(command)
            else {
                panic!("expected malformed shell command");
            };
            assert_eq!(position, expected_position);
            assert_eq!(kind, expected_kind);
        }
    }

    #[test]
    fn exec_state_root_chains() {
        let s = scratch()
            .unwrap()
            .run(shlex("echo hello").unwrap())
            .root()
            .unwrap();
        let _ = s.run(shlex("echo again").unwrap()).root().unwrap();
    }

    #[test]
    fn execop_rootfs_mount() {
        let base = scratch().unwrap().output().clone();
        let op = ExecOp::new(
            base,
            None,
            None,
            Vec::new(),
            RunOpts::default().with_arg("echo"),
        )
        .unwrap();
        let (exec, _) = serialize_exec_op(op);
        assert_eq!(exec.mounts[0].dest, "/");
        assert_eq!(exec.mounts[0].input, -1);
    }

    #[test]
    fn execop_env_merge_run_overrides_base() {
        let base = scratch().unwrap().output().clone();
        let run = RunOpts::default().with_arg("env").with_env("K", "V2");
        let op = ExecOp::new(
            base,
            None,
            None,
            vec![("K".to_string(), "V1".to_string())],
            run,
        )
        .unwrap();
        let (exec, _) = serialize_exec_op(op);
        let meta = exec.meta.expect("expected Meta");
        assert!(meta.env.contains(&"K=V2".to_string()));
        assert!(!meta.env.contains(&"K=V1".to_string()));
    }

    #[test]
    fn exec_env_merge_preserves_order_and_last_value() {
        let merged = merge_env(
            &[
                ("K".to_string(), "V1".to_string()),
                ("K".to_string(), "V2".to_string()),
            ],
            &[],
        );
        assert_eq!(merged, vec![("K".to_string(), "V2".to_string())]);

        let merged = merge_env(
            &[
                ("K".to_string(), "V1".to_string()),
                ("A".to_string(), "x".to_string()),
            ],
            &[("K".to_string(), "V3".to_string())],
        );
        assert_eq!(
            merged,
            vec![
                ("K".to_string(), "V3".to_string()),
                ("A".to_string(), "x".to_string())
            ]
        );
    }
}
