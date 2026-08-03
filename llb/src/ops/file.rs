//! File-operation types: copy, mkdir, mkfile, and friends.

use bollard_buildkit_proto::pb;

use crate::error::LlbError;
use crate::marshal::Digest;
use crate::metadata::{attr, cap, OpMetadata};
use crate::ops::{Context, Node, NodeRef, Operation, OperationOutput, OutputIdx};
use crate::state::State;

/// A single file-system action chained into a `FileOp`.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum FileAction {
    /// Copy files or directories from a source state.
    Copy {
        /// Source state.
        src: State,
        /// Path inside the source state.
        src_path: String,
        /// Destination path in the current state.
        dest_path: String,
        /// Copy options.
        info: CopyInfo,
    },
    /// Create a directory.
    Mkdir {
        /// Directory path.
        path: String,
        /// File mode.
        mode: u32,
        /// Create parent directories.
        parents: bool,
    },
    /// Create a file with the given contents.
    MkFile {
        /// File path.
        path: String,
        /// File mode.
        mode: u32,
        /// File contents.
        data: Vec<u8>,
    },
    /// Remove a file or directory.
    Rm {
        /// Path to remove.
        path: String,
        /// Allow the path to be absent.
        allow_not_found: bool,
        /// Allow wildcards.
        allow_wildcard: bool,
    },
    /// Create a symlink.
    Symlink {
        /// Target of the symlink.
        target: String,
        /// Path where the symlink is created.
        link_path: String,
    },
}

impl FileAction {
    /// Set `create_dest_path` on a [`FileAction::Copy`].
    pub fn with_create_dest_path(mut self, v: bool) -> Self {
        if let Self::Copy {
            src,
            src_path,
            dest_path,
            mut info,
        } = self
        {
            info.create_dest_path = v;
            self = Self::Copy {
                src,
                src_path,
                dest_path,
                info,
            };
        }
        self
    }

    /// Set `follow_symlinks` on a [`FileAction::Copy`].
    pub fn with_follow_symlinks(mut self, v: bool) -> Self {
        if let Self::Copy {
            src,
            src_path,
            dest_path,
            mut info,
        } = self
        {
            info.follow_symlinks = v;
            self = Self::Copy {
                src,
                src_path,
                dest_path,
                info,
            };
        }
        self
    }

    /// Set `copy_dir_contents_only` on a [`FileAction::Copy`].
    pub fn with_copy_dir_contents_only(mut self, v: bool) -> Self {
        if let Self::Copy {
            src,
            src_path,
            dest_path,
            mut info,
        } = self
        {
            info.copy_dir_contents_only = v;
            self = Self::Copy {
                src,
                src_path,
                dest_path,
                info,
            };
        }
        self
    }

    /// Set `allow_wildcard` on a [`FileAction::Copy`] or [`FileAction::Rm`].
    pub fn with_allow_wildcard(mut self, v: bool) -> Self {
        match &mut self {
            Self::Copy { info, .. } => info.allow_wildcard = v,
            Self::Rm {
                allow_wildcard: ref mut aw,
                ..
            } => *aw = v,
            _ => {}
        }
        self
    }

    /// Set `allow_not_found` on a [`FileAction::Rm`].
    pub fn with_allow_not_found(mut self, v: bool) -> Self {
        if let Self::Rm {
            path,
            allow_wildcard,
            ..
        } = self
        {
            self = Self::Rm {
                path,
                allow_not_found: v,
                allow_wildcard,
            };
        }
        self
    }

    /// Set `allow_empty_wildcard` on a [`FileAction::Copy`].
    pub fn with_allow_empty_wildcard(mut self, v: bool) -> Self {
        if let Self::Copy {
            src,
            src_path,
            dest_path,
            mut info,
        } = self
        {
            info.allow_empty_wildcard = v;
            self = Self::Copy {
                src,
                src_path,
                dest_path,
                info,
            };
        }
        self
    }

    /// Add an exclude pattern on a [`FileAction::Copy`].
    pub fn with_exclude_pattern<S: Into<String>>(mut self, p: S) -> Self {
        if let Self::Copy {
            src,
            src_path,
            dest_path,
            mut info,
        } = self
        {
            info.exclude_patterns.push(p.into());
            self = Self::Copy {
                src,
                src_path,
                dest_path,
                info,
            };
        }
        self
    }

    /// Set `make_parents` on a [`FileAction::Mkdir`].
    pub fn with_parents(mut self, v: bool) -> Self {
        if let Self::Mkdir {
            path,
            mode,
            parents: _,
        } = self
        {
            self = Self::Mkdir {
                path,
                mode,
                parents: v,
            };
        }
        self
    }
}

/// Options for a copy action.
#[derive(Clone, Debug, Default)]
pub struct CopyInfo {
    /// Create destination parent directories if missing.
    pub create_dest_path: bool,
    /// Follow symlinks in the source.
    pub follow_symlinks: bool,
    /// Copy only the contents of a directory, not the directory itself.
    pub copy_dir_contents_only: bool,
    /// Allow wildcard patterns in source paths.
    pub allow_wildcard: bool,
    /// Allow wildcard patterns that match nothing.
    pub allow_empty_wildcard: bool,
    /// Patterns to exclude.
    pub exclude_patterns: Vec<String>,
}

impl CopyInfo {
    /// Create default copy info.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set `create_dest_path`.
    pub fn with_create_dest_path(mut self, v: bool) -> Self {
        self.create_dest_path = v;
        self
    }

    /// Set `follow_symlinks`.
    pub fn with_follow_symlinks(mut self, v: bool) -> Self {
        self.follow_symlinks = v;
        self
    }

    /// Set `copy_dir_contents_only`.
    pub fn with_copy_dir_contents_only(mut self, v: bool) -> Self {
        self.copy_dir_contents_only = v;
        self
    }

    /// Set `allow_wildcard`.
    pub fn with_allow_wildcard(mut self, v: bool) -> Self {
        self.allow_wildcard = v;
        self
    }

    /// Set `allow_empty_wildcard`.
    pub fn with_allow_empty_wildcard(mut self, v: bool) -> Self {
        self.allow_empty_wildcard = v;
        self
    }

    /// Add an exclude pattern.
    pub fn with_exclude_pattern<S: Into<String>>(mut self, p: S) -> Self {
        self.exclude_patterns.push(p.into());
        self
    }
}

/// Options for a `State::file` call.
#[derive(Clone, Debug, Default)]
pub struct FileOpts {
    /// Custom name for the file operation vertex.
    pub custom_name: Option<String>,
    /// Ignore the build cache for this vertex.
    pub ignore_cache: bool,
}

impl FileOpts {
    /// Create default file options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a custom name.
    pub fn with_custom_name<S: Into<String>>(mut self, name: S) -> Self {
        self.custom_name = Some(name.into());
        self
    }

    /// Set ignore cache.
    pub fn with_ignore_cache(mut self, v: bool) -> Self {
        self.ignore_cache = v;
        self
    }
}

/// Create a copy file action.
pub fn copy<S1: Into<String>, S2: Into<String>>(
    src: State,
    src_path: S1,
    dest_path: S2,
) -> FileAction {
    FileAction::Copy {
        src,
        src_path: src_path.into(),
        dest_path: dest_path.into(),
        info: CopyInfo::default(),
    }
}

/// Create a mkdir file action.
pub fn mkdir<S: Into<String>>(path: S, mode: u32) -> FileAction {
    FileAction::Mkdir {
        path: path.into(),
        mode,
        parents: false,
    }
}

/// Create a mkfile file action.
pub fn mkfile<S: Into<String>>(path: S, mode: u32, data: impl Into<Vec<u8>>) -> FileAction {
    FileAction::MkFile {
        path: path.into(),
        mode,
        data: data.into(),
    }
}

/// Create a remove file action.
pub fn rm<S: Into<String>>(path: S) -> FileAction {
    FileAction::Rm {
        path: path.into(),
        allow_not_found: false,
        allow_wildcard: false,
    }
}

/// Create a symlink file action.
pub fn symlink<S1: Into<String>, S2: Into<String>>(target: S1, link_path: S2) -> FileAction {
    FileAction::Symlink {
        target: target.into(),
        link_path: link_path.into(),
    }
}

/// A fully assembled file operation.
#[derive(Clone, Debug)]
pub(crate) struct FileOp {
    base: OperationOutput,
    action: FileAction,
    opts: FileOpts,
    metadata: OpMetadata,
}

impl FileOp {
    /// Build a new file operation from the base state, a single action, and
    /// options.
    ///
    /// The actual protobuf bytes are computed at marshal time so that the
    /// active worker constraints affect the content digest.
    pub(crate) fn new(
        base: OperationOutput,
        action: FileAction,
        opts: FileOpts,
    ) -> Result<Self, LlbError> {
        let metadata = build_file_metadata(&action, &opts);
        Ok(Self {
            base,
            action,
            opts,
            metadata,
        })
    }
}

impl Operation for FileOp {
    fn serialize(&self, ctx: &mut Context) -> Result<NodeRef, LlbError> {
        let base_empty = self.base.is_empty();
        let mut inputs: Vec<OperationOutput> = Vec::new();
        let mut input_keys: Vec<(Digest, OutputIdx)> = Vec::new();

        if !base_empty {
            let node_ref = ctx.register(&self.base)?;
            inputs.push(self.base.clone());
            input_keys.push((node_ref.digest().clone(), node_ref.index()));
        }

        let pb_action =
            build_pb_file_action(&self.action, base_empty, &mut inputs, &mut input_keys, ctx)?;
        let file_op = pb::FileOp {
            actions: vec![pb_action],
        };

        let pb_inputs: Vec<pb::Input> = input_keys
            .into_iter()
            .map(|(digest, index)| pb::Input {
                digest: digest.as_str().to_string(),
                index: index.0 as i64,
            })
            .collect();

        let pb_op = pb::Op {
            inputs: pb_inputs,
            platform: None,
            constraints: Some(pb::WorkerConstraints {
                filter: ctx.worker_filters().to_vec(),
            }),
            op: Some(pb::op::Op::File(file_op)),
        };

        let (digest, bytes) = crate::marshal::encode_and_hash(&pb_op)?;
        Ok(ctx.insert_node(Node {
            bytes,
            digest,
            metadata: self.metadata.clone(),
        }))
    }
}

fn build_pb_file_action(
    action: &FileAction,
    base_empty: bool,
    inputs: &mut Vec<OperationOutput>,
    input_keys: &mut Vec<(Digest, OutputIdx)>,
    ctx: &mut Context,
) -> Result<pb::FileAction, LlbError> {
    let base_input = if base_empty { -1 } else { 0 };
    match action {
        FileAction::Copy {
            src,
            src_path,
            dest_path,
            info,
        } => {
            let secondary = if src.output().is_empty() {
                -1
            } else {
                let output = src.output().clone();
                let node_ref = ctx.register(&output)?;
                let key = (node_ref.digest().clone(), node_ref.index());
                if let Some(pos) = input_keys.iter().position(|k| *k == key) {
                    pos as i64
                } else {
                    let pos = inputs.len() as i64;
                    inputs.push(output);
                    input_keys.push(key);
                    pos
                }
            };

            let copy = pb::FileActionCopy {
                src: src_path.clone(),
                dest: dest_path.clone(),
                owner: None,
                mode: -1,
                follow_symlink: info.follow_symlinks,
                dir_copy_contents: info.copy_dir_contents_only,
                attempt_unpack_docker_compatibility: false,
                create_dest_path: info.create_dest_path,
                allow_wildcard: info.allow_wildcard,
                allow_empty_wildcard: info.allow_empty_wildcard,
                timestamp: -1,
                include_patterns: Vec::new(),
                exclude_patterns: info.exclude_patterns.clone(),
                always_replace_existing_dest_paths: false,
                mode_str: String::new(),
                required_paths: Vec::new(),
            };

            Ok(pb::FileAction {
                input: base_input,
                secondary_input: secondary,
                output: 0,
                action: Some(pb::file_action::Action::Copy(copy)),
            })
        }
        FileAction::Mkdir {
            path,
            mode,
            parents,
        } => {
            let mkdir = pb::FileActionMkDir {
                path: path.clone(),
                mode: *mode as i32,
                make_parents: *parents,
                owner: None,
                timestamp: -1,
            };
            Ok(pb::FileAction {
                input: base_input,
                secondary_input: -1,
                output: 0,
                action: Some(pb::file_action::Action::Mkdir(mkdir)),
            })
        }
        FileAction::MkFile { path, mode, data } => {
            let mkfile = pb::FileActionMkFile {
                path: path.clone(),
                mode: *mode as i32,
                data: data.clone(),
                owner: None,
                timestamp: -1,
            };
            Ok(pb::FileAction {
                input: base_input,
                secondary_input: -1,
                output: 0,
                action: Some(pb::file_action::Action::Mkfile(mkfile)),
            })
        }
        FileAction::Rm {
            path,
            allow_not_found,
            allow_wildcard,
        } => {
            let rm = pb::FileActionRm {
                path: path.clone(),
                allow_not_found: *allow_not_found,
                allow_wildcard: *allow_wildcard,
            };
            Ok(pb::FileAction {
                input: base_input,
                secondary_input: -1,
                output: 0,
                action: Some(pb::file_action::Action::Rm(rm)),
            })
        }
        FileAction::Symlink { target, link_path } => {
            let symlink = pb::FileActionSymlink {
                oldpath: target.clone(),
                newpath: link_path.clone(),
                owner: None,
                timestamp: -1,
            };
            Ok(pb::FileAction {
                input: base_input,
                secondary_input: -1,
                output: 0,
                action: Some(pb::file_action::Action::Symlink(symlink)),
            })
        }
    }
}

fn build_file_metadata(_action: &FileAction, opts: &FileOpts) -> OpMetadata {
    // The pinned Go oracle advertises file.base on these supported actions;
    // action-specific caps are not present in its generated metadata.
    let mut metadata = OpMetadata::default();
    metadata.caps.insert(cap::CAP_FILE_BASE.to_string());

    if opts.ignore_cache {
        metadata.ignore_cache = true;
    }

    if let Some(name) = &opts.custom_name {
        metadata
            .description
            .insert(attr::DESCRIPTION_NAME.to_string(), name.clone());
    }

    metadata
}
