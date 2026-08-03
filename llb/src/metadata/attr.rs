//! Well-known attribute keys used in [`SourceOp`](bollard_buildkit_proto::pb::SourceOp)
//! attrs and operation metadata descriptions.
//!
//! These values are kept in sync with `moby/buildkit` (`solver/pb/attr.go` and
//! `client/llb/state.go`).

// Local source attributes ---------------------------------------------------

/// `local.followpaths`
pub const LOCAL_FOLLOW_PATHS: &str = "local.followpaths";
/// `local.session`
pub const LOCAL_SESSION_ID: &str = "local.session";
/// `local.sharedkeyhint`
pub const LOCAL_SHARED_KEY_HINT: &str = "local.sharedkeyhint";
/// `local.unique`
pub const LOCAL_UNIQUE_ID: &str = "local.unique";
/// `local.includepattern`
pub const LOCAL_INCLUDE_PATTERNS: &str = "local.includepattern";
/// `local.excludepatterns`
pub const LOCAL_EXCLUDE_PATTERNS: &str = "local.excludepatterns";

// Image source attributes ---------------------------------------------------

/// `image.resolvemode`
pub const IMAGE_RESOLVE_MODE: &str = "image.resolvemode";
/// `default` image resolve mode.
pub const IMAGE_RESOLVE_MODE_DEFAULT: &str = "default";
/// `pull` image resolve mode.
pub const IMAGE_RESOLVE_MODE_FORCE_PULL: &str = "pull";
/// `local` image resolve mode.
pub const IMAGE_RESOLVE_MODE_PREFER_LOCAL: &str = "local";
/// `image.layerlimit`
pub const IMAGE_LAYER_LIMIT: &str = "image.layerlimit";
/// `image.checksum`
pub const IMAGE_CHECKSUM: &str = "image.checksum";

// Description / progress attributes -----------------------------------------

/// `llb.customname` — the description key set by `WithCustomName`.
pub const DESCRIPTION_NAME: &str = "llb.customname";
