//! The central LLB builder types: [`State`], [`ExecState`], and [`Constraints`].

use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::Arc;

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
        Self::with_constraints(output, Constraints::default())
    }

    pub(crate) fn with_constraints(output: OperationOutput, constraints: Constraints) -> Self {
        Self {
            output,
            constraints,
        }
    }

    /// Access the operation output backing this state.
    pub(crate) fn output(&self) -> &OperationOutput {
        &self.output
    }

    pub(crate) fn constraints(&self) -> &Constraints {
        &self.constraints
    }

    pub(crate) fn cwd(&self) -> Option<&str> {
        self.constraints.cwd.as_deref()
    }

    /// Set the working directory for subsequent exec steps.
    pub fn dir<S: Into<String>>(mut self, path: S) -> Self {
        let path = path.into();
        self.constraints.cwd = Some(if crate::path::is_abs(&path) {
            path
        } else {
            let previous = self.constraints.cwd.clone().unwrap_or_default();
            crate::path::join(&[if previous.is_empty() { "/" } else { &previous }, &path])
        });
        self
    }

    /// Add an environment variable for subsequent exec steps.
    pub fn add_env<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        replace_env(&mut self.constraints.env, key.into(), value.into());
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
    pub fn file(self, action: FileAction, opts: impl Into<FileOpts>) -> Result<Self, LlbError> {
        let opts = opts.into();
        let file_op = FileOp::new(self.output, action, opts, self.constraints.cwd.clone())?;
        Ok(Self {
            output: OperationOutput::Owned(Arc::new(file_op)),
            constraints: self.constraints,
        })
    }

    /// Return the platform constraint for this state, defaulting to
    /// `linux/amd64` when none is set.
    pub fn platform(&self) -> Platform {
        self.constraints
            .platform
            .clone()
            .unwrap_or_else(|| Platform::LINUX_AMD64.clone())
    }

    /// Set the platform constraint for operations subsequently created from
    /// this state.
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.constraints.platform = Some(platform);
        self
    }

    /// Add a worker constraint filter for operations created from this state.
    pub fn with_worker_filter<S: Into<String>>(mut self, filter: S) -> Self {
        self.constraints.worker_filters.push(filter.into());
        self
    }

    /// Marshal this state into a [`Definition`].
    pub fn marshal(&self, opts: MarshalOpts) -> Result<Definition, LlbError> {
        if self.output.is_empty() {
            return Ok(Definition {
                def: Vec::new(),
                metadata: BTreeMap::new(),
                source: None,
                root: None,
            });
        }
        let worker_filters = self
            .constraints
            .worker_filters
            .iter()
            .cloned()
            .chain(opts.worker_filters.iter().cloned())
            .collect();
        // MarshalOpts supplies the graph-wide default. State-local platforms
        // are carried by the operations created from this state instead.
        let mut ctx = Context::new(opts.platform.clone(), worker_filters);
        let root_ref = ctx.register(&self.output)?;
        let wrapper_ref =
            ctx.append_wrapper(root_ref.clone(), self.constraints.custom_name.as_deref())?;
        Ok(ctx.finalize(
            wrapper_ref.digest().clone(),
            Some(root_ref.digest().clone()),
        ))
    }

    /// Set a custom name for the operation that produces this state.
    pub fn with_custom_name<S: Into<String>>(mut self, name: S) -> Self {
        self.constraints.custom_name = Some(name.into());
        self
    }

    /// Construct a scratch (empty) state.
    pub fn scratch() -> Result<Self, LlbError> {
        crate::scratch()
    }
}

/// Constraints carried by a state and applied to subsequently created
/// operations.
#[derive(Clone, Debug, Default)]
pub struct Constraints {
    platform: Option<Platform>,
    worker_filters: Vec<String>,
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

    /// Add a worker constraint filter.
    pub fn with_worker_filter<S: Into<String>>(mut self, filter: S) -> Self {
        self.worker_filters.push(filter.into());
        self
    }
}

/// Options passed to [`State::marshal`].
#[derive(Clone, Debug)]
pub struct MarshalOpts {
    /// Graph-wide default platform for real operation vertices without a
    /// state-local platform.
    pub platform: Option<Platform>,
    /// Worker constraint filters applied to real operation vertices.
    pub worker_filters: Vec<String>,
}

impl Default for MarshalOpts {
    fn default() -> Self {
        Self {
            // Match Go's `llb.State.Marshal(ctx)`, which defaults to linux/amd64. This keeps
            // wrapper digests identical across SDKs and avoids cross-SDK cache fragmentation.
            platform: Some(Platform::LINUX_AMD64.clone()),
            worker_filters: Vec::new(),
        }
    }
}

impl MarshalOpts {
    /// Marshal with the `linux/amd64` platform constraint.
    pub fn linux_amd64() -> Self {
        Self::default()
    }

    /// Marshal with the given platform constraint.
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Marshal with the given worker constraint filter.
    pub fn with_worker_filter<S: Into<String>>(mut self, filter: S) -> Self {
        self.worker_filters.push(filter.into());
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
    pub fn root(self) -> Result<State, LlbError> {
        let exec_op = ExecOp::new(
            self.base.output,
            self.base.constraints.platform.clone(),
            self.base.constraints.cwd.clone(),
            self.base.constraints.env.clone(),
            self.run,
        )?;
        Ok(State {
            output: OperationOutput::Owned(Arc::new(exec_op)),
            constraints: self.base.constraints,
        })
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
        replace_env(&mut self.env, key.into(), value.into());
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

fn replace_env(env: &mut Vec<(String, String)>, key: String, value: String) {
    if let Some((_, existing_value)) = env
        .iter_mut()
        .find(|(existing_key, _)| existing_key.as_str() == key)
    {
        *existing_value = value;
    } else {
        env.push((key, value));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_accumulates_relative_paths_like_go() {
        assert_eq!(State::scratch().unwrap().dir("foo").cwd(), Some("/foo"));
        assert_eq!(
            State::scratch().unwrap().dir("/work").dir("foo").cwd(),
            Some("/work/foo")
        );
        assert_eq!(
            State::scratch().unwrap().dir("/work").dir("/abs").cwd(),
            Some("/abs")
        );
        assert_eq!(
            State::scratch().unwrap().dir("a").dir("../b").cwd(),
            Some("/b")
        );
    }
}
