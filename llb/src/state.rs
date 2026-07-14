//! The central LLB builder types: [`State`], [`ExecState`], and [`Constraints`].

use std::fmt::Display;
use std::sync::Arc;

use bollard_buildkit_proto::pb;

use crate::definition::Definition;
use crate::error::LlbError;
use crate::ops::exec::{
    AddSecret, CacheSharingMode, ExecOp, Mount, MountType, NetMode, SecurityMode, Shlex,
};
use crate::ops::file::{FileAction, FileOp, FileOpts};
use crate::ops::{Context, OperationOutput};
use crate::platform::Platform;

/// A filesystem state in the LLB graph. Cheaply cloneable (Arc-backed).
#[derive(Clone, Debug)]
pub struct State {
    output: OperationOutput,
    constraints: Constraints,
}

impl State {
    pub(crate) fn new(output: OperationOutput) -> Self {
        Self {
            output,
            constraints: Constraints::default(),
        }
    }

    /// Access the operation output backing this state.
    pub(crate) fn output(&self) -> &OperationOutput {
        &self.output
    }

    /// Set the working directory for subsequent exec steps.
    pub fn dir<S: Into<String>>(mut self, path: S) -> Self {
        self.constraints.cwd = Some(path.into());
        self
    }

    /// Add an environment variable for subsequent exec steps.
    pub fn add_env<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.constraints.env.push((key.into(), value.into()));
        self
    }

    /// Add an environment variable whose value is formatted.
    pub fn add_envf<K: Into<String>, V: Display>(mut self, key: K, value: V) -> Self {
        self.constraints.env.push((key.into(), format!("{value}")));
        self
    }

    /// Start a new run step from this state.
    pub fn run(self, opts: impl Into<RunOpts>) -> ExecState {
        ExecState {
            base: self,
            run: opts.into(),
        }
    }

    /// Apply a file action to this state.
    pub fn file(self, action: FileAction, opts: impl Into<FileOpts>) -> Self {
        let opts = opts.into();
        let file_op = FileOp::new(self.output, action, opts);
        Self {
            output: OperationOutput::Owned(Arc::new(file_op)),
            constraints: self.constraints,
        }
    }

    /// Return the platform constraint for this state.
    pub fn platform(&self) -> Platform {
        self.constraints
            .platform
            .clone()
            .unwrap_or_else(|| Platform::LINUX_AMD64.clone())
    }

    /// Marshal this state into a [`Definition`].
    pub fn marshal(&self, _opts: MarshalOpts) -> Result<Definition, LlbError> {
        let mut ctx = Context::new();
        let root_ref = ctx.register(&self.output)?;
        let wrapper_ref = ctx.append_wrapper(root_ref)?;
        Ok(ctx.finalize(wrapper_ref.digest().clone()))
    }

    /// Set a custom name for the operation that produces this state.
    pub fn with_custom_name<S: Into<String>>(mut self, name: S) -> Self {
        self.constraints.custom_name = Some(name.into());
        self
    }

    /// Construct a scratch (empty) state.
    pub fn scratch() -> Self {
        crate::scratch()
    }
}

/// Constraints overlaid onto the root wrapper vertex at marshal time.
#[derive(Clone, Debug, Default)]
pub struct Constraints {
    platform: Option<Platform>,
    cwd: Option<String>,
    env: Vec<(String, String)>,
    custom_name: Option<String>,
}

impl Constraints {
    /// Set the platform constraint.
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Convert to a protobuf [`pb::Platform`].
    pub fn to_pb_platform(&self) -> Option<pb::Platform> {
        self.platform.clone().map(Into::into)
    }
}

/// Options passed to [`State::marshal`].
#[derive(Clone, Debug, Default)]
pub struct MarshalOpts {
    /// Platform constraint applied at the root wrapper vertex.
    pub platform: Option<Platform>,
}

impl MarshalOpts {
    /// Marshal with the `linux/amd64` platform constraint.
    pub fn linux_amd64() -> Self {
        Self {
            platform: Some(Platform::LINUX_AMD64.clone()),
        }
    }

    /// Marshal with the given platform constraint.
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }
}

/// A run-step builder.
#[derive(Clone, Debug)]
pub struct ExecState {
    base: State,
    /// Accumulated run-time options. Mutated by [`RunOpt`] implementations.
    pub(crate) run: RunOpts,
}

impl ExecState {
    /// Return the post-exec state.
    pub fn root(self) -> State {
        let exec_op = ExecOp::new(
            self.base.output,
            self.base.constraints.cwd.clone(),
            self.base.constraints.env.clone(),
            self.run,
        );
        State {
            output: OperationOutput::Owned(Arc::new(exec_op)),
            constraints: self.base.constraints,
        }
    }

    /// Add a bind mount from a source state.
    pub fn add_mount<S: Into<String>>(mut self, target: S, src: State) -> Self {
        self.run.mounts.push(Mount {
            target: target.into(),
            source: Some(src),
            mount_type: MountType::Bind,
            readonly: false,
            output: None,
        });
        self
    }

    /// Add a scratch mount at the given target.
    pub fn add_mount_scratch<S: Into<String>>(mut self, target: S) -> Self {
        self.run.mounts.push(Mount {
            target: target.into(),
            source: None,
            mount_type: MountType::Scratch,
            readonly: false,
            output: None,
        });
        self
    }

    /// Add a cache mount at the given target.
    pub fn add_mount_cache<S: Into<String>>(
        mut self,
        target: S,
        id: S,
        mode: CacheSharingMode,
    ) -> Self {
        self.run.mounts.push(Mount {
            target: target.into(),
            source: None,
            mount_type: MountType::Cache {
                id: id.into(),
                mode,
            },
            readonly: false,
            output: None,
        });
        self
    }

    /// Add a secret mount and/or environment variable.
    pub fn add_secret<S: Into<String>>(mut self, id: S, opts: impl Into<AddSecret>) -> Self {
        let mut opts = opts.into();
        opts.id = id.into();
        self.run.secrets.push(opts);
        self
    }

    /// Set a custom name for this exec step.
    pub fn with_custom_name<S: Into<String>>(mut self, name: S) -> Self {
        self.run.custom_name = Some(name.into());
        self
    }

    /// Retrieve a mount by target path.
    pub fn get_mount(&self, target: &str) -> Option<&Mount> {
        self.run.mounts.iter().find(|m| m.target == target)
    }
}

/// Accumulated options for an exec step.
#[derive(Clone, Debug, Default)]
pub struct RunOpts {
    /// Command arguments.
    pub args: Vec<String>,
    /// Environment variables.
    pub env: Vec<(String, String)>,
    /// Mounts.
    pub mounts: Vec<Mount>,
    /// Secret mounts / env vars.
    pub secrets: Vec<AddSecret>,
    /// Custom name for the exec vertex.
    pub custom_name: Option<String>,
    /// Network mode.
    pub net: NetMode,
    /// Security mode.
    pub security: SecurityMode,
    /// Ignore cache.
    pub ignore_cache: bool,
}

impl RunOpts {
    /// Create empty run options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a command argument.
    pub fn with_arg<S: Into<String>>(mut self, arg: S) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add an environment variable.
    pub fn with_env<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Set a custom name.
    pub fn with_custom_name<S: Into<String>>(mut self, name: S) -> Self {
        self.custom_name = Some(name.into());
        self
    }

    /// Set the network mode.
    pub fn with_net(mut self, net: NetMode) -> Self {
        self.net = net;
        self
    }

    /// Set the security mode.
    pub fn with_security(mut self, security: SecurityMode) -> Self {
        self.security = security;
        self
    }

    /// Add a bind mount.
    pub fn with_mount<S: Into<String>>(mut self, target: S, src: crate::State) -> Self {
        self.mounts.push(Mount {
            target: target.into(),
            source: Some(src),
            mount_type: MountType::Bind,
            readonly: false,
            output: None,
        });
        self
    }

    /// Add a secret.
    pub fn with_secret(mut self, opts: impl Into<AddSecret>) -> Self {
        self.secrets.push(opts.into());
        self
    }
}

/// Marker trait for types that can be applied to an [`ExecState`].
pub trait RunOpt {
    /// Apply this option to the exec state.
    fn apply(self, exec: &mut ExecState);
}

impl From<Shlex> for RunOpts {
    fn from(value: Shlex) -> Self {
        Self {
            args: value.args,
            ..Default::default()
        }
    }
}
