//! File-operation types: copy, mkdir, mkfile, and friends.
//!
//! This module is scaffolded in Phase 2; the `Operation` serialization for
//! [`FileOp`](bollard_buildkit_proto::pb::FileOp) is implemented in Phase 3.

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
