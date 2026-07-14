//! Execution-operation types: mounts, secrets, cache mounts, and run options.
//!
//! This module is scaffolded in Phase 2; the `Operation` serialization for
//! [`ExecOp`] is implemented in Phase 3.

use crate::state::State;

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

/// Network mode for an exec step.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NetMode {
    /// No network access.
    #[default]
    None,
    /// Use the host network namespace.
    Host,
    /// Use a sandboxed network (default for most builds).
    Sandbox,
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

/// Command arguments for an exec step.
#[derive(Clone, Debug)]
pub struct Shlex {
    /// Argument vector.
    pub args: Vec<String>,
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
