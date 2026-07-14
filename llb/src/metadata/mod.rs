//! Per-operation metadata, capability constants, and well-known attribute keys.

pub mod attr;
pub mod cap;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use bollard_buildkit_proto::pb;

/// Rich metadata attached to an LLB operation vertex.
///
/// Mirrors [`pb::OpMetadata`] but uses ordered collections for deterministic
/// serialization and exposes the fields that prior art dropped.
#[derive(Clone, Debug, Default)]
pub struct OpMetadata {
    /// Capability IDs required by this operation.
    pub caps: BTreeSet<String>,
    /// Ignore the build cache for this vertex.
    pub ignore_cache: bool,
    /// Free-form description map, typically containing the custom name under
    /// [`attr::DESCRIPTION_NAME`].
    pub description: BTreeMap<String, String>,
    /// Whether this vertex should be considered for export cache.
    pub export_cache: Option<bool>,
    /// Optional progress group for UI grouping.
    pub progress_group: Option<pb::ProgressGroup>,
}

impl From<OpMetadata> for pb::OpMetadata {
    fn from(value: OpMetadata) -> Self {
        Self {
            ignore_cache: value.ignore_cache,
            description: value.description.into_iter().collect::<HashMap<_, _>>(),
            export_cache: value.export_cache.map(|v| pb::ExportCache { value: v }),
            caps: value
                .caps
                .into_iter()
                .map(|cap| (cap, true))
                .collect::<HashMap<_, _>>(),
            progress_group: value.progress_group,
        }
    }
}
