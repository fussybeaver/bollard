//! Capability IDs advertised in [`OpMetadata`](crate::metadata::OpMetadata).
//!
//! These strings must match `moby/buildkit`'s `solver/pb/caps.go` exactly.
//! The full upstream list covers git, http, oci, exporter and gc features that
//! are outside the current `bollard-llb` scope; this module exposes only the
//! subset of capability IDs the crate actually emits.

// Source -------------------------------------------------------------------

/// `source.image`
pub const CAP_SOURCE_IMAGE: &str = "source.image";
/// `source.image.resolvemode`
pub const CAP_SOURCE_IMAGE_RESOLVE_MODE: &str = "source.image.resolvemode";
/// `source.image.layerlimit`
pub const CAP_SOURCE_IMAGE_LAYER_LIMIT: &str = "source.image.layerlimit";
/// `source.image.checksum`
pub const CAP_SOURCE_IMAGE_CHECKSUM: &str = "source.image.checksum";

/// `source.local`
pub const CAP_SOURCE_LOCAL: &str = "source.local";
/// `source.local.unique`
pub const CAP_SOURCE_LOCAL_UNIQUE: &str = "source.local.unique";
/// `source.local.sessionid`
pub const CAP_SOURCE_LOCAL_SESSION_ID: &str = "source.local.sessionid";
/// `source.local.includepatterns`
pub const CAP_SOURCE_LOCAL_INCLUDE_PATTERNS: &str = "source.local.includepatterns";
/// `source.local.followpaths`
pub const CAP_SOURCE_LOCAL_FOLLOW_PATHS: &str = "source.local.followpaths";
/// `source.local.excludepatterns`
pub const CAP_SOURCE_LOCAL_EXCLUDE_PATTERNS: &str = "source.local.excludepatterns";
/// `source.local.sharedkeyhint`
pub const CAP_SOURCE_LOCAL_SHARED_KEY_HINT: &str = "source.local.sharedkeyhint";

// Exec metadata ------------------------------------------------------------

/// `exec.meta.base`
pub const CAP_EXEC_META_BASE: &str = "exec.meta.base";
/// `exec.meta.network`
pub const CAP_EXEC_META_NETWORK: &str = "exec.meta.network";
/// `exec.meta.security`
pub const CAP_EXEC_META_SECURITY: &str = "exec.meta.security";

// Exec mounts --------------------------------------------------------------

/// `exec.mount.bind`
pub const CAP_EXEC_MOUNT_BIND: &str = "exec.mount.bind";
/// `exec.mount.cache`
pub const CAP_EXEC_MOUNT_CACHE: &str = "exec.mount.cache";
/// `exec.mount.cache.sharing`
pub const CAP_EXEC_MOUNT_CACHE_SHARING: &str = "exec.mount.cache.sharing";
/// `exec.mount.secret`
pub const CAP_EXEC_MOUNT_SECRET: &str = "exec.mount.secret";
/// `exec.mount.ssh`
pub const CAP_EXEC_MOUNT_SSH: &str = "exec.mount.ssh";

// Exec other ---------------------------------------------------------------

/// `exec.secretenv`
pub const CAP_EXEC_SECRET_ENV: &str = "exec.secretenv";

// File ----------------------------------------------------------------------

/// `file.base`
pub const CAP_FILE_BASE: &str = "file.base";

// Constraints / platform / meta --------------------------------------------

/// `constraints`
pub const CAP_CONSTRAINTS: &str = "constraints";
/// `platform`
pub const CAP_PLATFORM: &str = "platform";

/// `meta.ignorecache`
pub const CAP_META_IGNORE_CACHE: &str = "meta.ignorecache";
/// `meta.description`
pub const CAP_META_DESCRIPTION: &str = "meta.description";
/// `meta.exportcache`
pub const CAP_META_EXPORT_CACHE: &str = "meta.exportcache";

// Composite ops ------------------------------------------------------------

/// `mergeop`
pub const CAP_MERGE_OP: &str = "mergeop";
