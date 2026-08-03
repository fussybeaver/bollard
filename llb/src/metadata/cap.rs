//! Capability IDs advertised in [`OpMetadata`](crate::metadata::OpMetadata).
//!
//! These strings must match `moby/buildkit`'s `solver/pb/caps.go` exactly.
//! The full upstream list covers git, http, oci, exporter and gc features that
//! are outside the current `bollard-llb` scope; this module exposes the subset
//! used by the crate plus a foundation for common ops.

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
/// `source.local.differ`
pub const CAP_SOURCE_LOCAL_DIFFER: &str = "source.local.differ";
/// `source.local.metadatatransfer`
pub const CAP_SOURCE_LOCAL_METADATA_TRANSFER: &str = "source.local.metadatatransfer";

// Exec metadata ------------------------------------------------------------

/// `exec.meta.base`
pub const CAP_EXEC_META_BASE: &str = "exec.meta.base";
/// `exec.meta.cgroup.parent`
pub const CAP_EXEC_META_CGROUP_PARENT: &str = "exec.meta.cgroup.parent";
/// `exec.meta.network`
pub const CAP_EXEC_META_NETWORK: &str = "exec.meta.network";
/// `exec.meta.network.proxy`
pub const CAP_EXEC_META_NETWORK_PROXY: &str = "exec.meta.network.proxy";
/// `exec.meta.proxyenv`
pub const CAP_EXEC_META_PROXY: &str = "exec.meta.proxyenv";
/// `exec.meta.security`
pub const CAP_EXEC_META_SECURITY: &str = "exec.meta.security";
/// `exec.meta.security.devices.v1`
pub const CAP_EXEC_META_SECURITY_DEVICE_WHITELIST_V1: &str = "exec.meta.security.devices.v1";
/// `exec.meta.setsdefaultpath`
pub const CAP_EXEC_META_SETS_DEFAULT_PATH: &str = "exec.meta.setsdefaultpath";
/// `exec.meta.ulimit`
pub const CAP_EXEC_META_ULIMIT: &str = "exec.meta.ulimit";
/// `exec.meta.cdi`
pub const CAP_EXEC_META_CDI: &str = "exec.meta.cdi";
/// `exec.meta.removemountstubs.recursive`
pub const CAP_EXEC_META_REMOVE_MOUNT_STUBS_RECURSIVE: &str = "exec.meta.removemountstubs.recursive";
/// `exec.meta.linux.resources`
pub const CAP_EXEC_META_LINUX_RESOURCES: &str = "exec.meta.linux.resources";

// Exec mounts --------------------------------------------------------------

/// `exec.mount.bind`
pub const CAP_EXEC_MOUNT_BIND: &str = "exec.mount.bind";
/// `exec.mount.bind.readwrite-nooutput`
pub const CAP_EXEC_MOUNT_BIND_READ_WRITE_NO_OUTPUT: &str = "exec.mount.bind.readwrite-nooutput";
/// `exec.mount.cache`
pub const CAP_EXEC_MOUNT_CACHE: &str = "exec.mount.cache";
/// `exec.mount.cache.sharing`
pub const CAP_EXEC_MOUNT_CACHE_SHARING: &str = "exec.mount.cache.sharing";
/// `exec.mount.selector`
pub const CAP_EXEC_MOUNT_SELECTOR: &str = "exec.mount.selector";
/// `exec.mount.tmpfs`
pub const CAP_EXEC_MOUNT_TMPFS: &str = "exec.mount.tmpfs";
/// `exec.mount.tmpfs.size`
pub const CAP_EXEC_MOUNT_TMPFS_SIZE: &str = "exec.mount.tmpfs.size";
/// `exec.mount.secret`
pub const CAP_EXEC_MOUNT_SECRET: &str = "exec.mount.secret";
/// `exec.mount.ssh`
pub const CAP_EXEC_MOUNT_SSH: &str = "exec.mount.ssh";
/// `exec.mount.cache.content`
pub const CAP_EXEC_MOUNT_CONTENT_CACHE: &str = "exec.mount.cache.content";

// Exec other ---------------------------------------------------------------

/// `exec.cgroup`
pub const CAP_EXEC_CGROUPS_MOUNTED: &str = "exec.cgroup";
/// `exec.secretenv`
pub const CAP_EXEC_SECRET_ENV: &str = "exec.secretenv";
/// `exec.validexitcode`
pub const CAP_EXEC_VALID_EXIT_CODE: &str = "exec.validexitcode";

// File ----------------------------------------------------------------------

/// `file.base`
pub const CAP_FILE_BASE: &str = "file.base";
/// `file.rm.wildcard`
pub const CAP_FILE_RM_WILDCARD: &str = "file.rm.wildcard";
/// `file.copy.includeexcludepatterns`
pub const CAP_FILE_COPY_INCLUDE_EXCLUDE_PATTERNS: &str = "file.copy.includeexcludepatterns";
/// `file.copy.requiredpaths`
pub const CAP_FILE_COPY_REQUIRED_PATHS: &str = "file.copy.requiredpaths";
/// `file.rm.nofollowsymlink`
pub const CAP_FILE_RM_NO_FOLLOW_SYMLINK: &str = "file.rm.nofollowsymlink";
/// `file.copy.alwaysreplaceexistingdestpaths`
pub const CAP_FILE_COPY_ALWAYS_REPLACE_EXISTING_DEST_PATHS: &str =
    "file.copy.alwaysreplaceexistingdestpaths";
/// `file.copy.modestring`
pub const CAP_FILE_COPY_MODE_STRING_FORMAT: &str = "file.copy.modestring";
/// `file.symlink.create`
pub const CAP_FILE_SYMLINK_CREATE: &str = "file.symlink.create";

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
/// `diffop`
pub const CAP_DIFF_OP: &str = "diffop";
/// `passthroughop`
pub const CAP_PASSTHROUGH_OP: &str = "passthroughop";
