//! Per-operation metadata, capability constants, and well-known attribute keys.

pub mod attr;
pub mod cap;

use std::collections::{BTreeMap, BTreeSet};

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
            description: value.description,
            export_cache: value.export_cache.map(|v| pb::ExportCache { value: v }),
            caps: value
                .caps
                .into_iter()
                .map(|cap| (cap, true))
                .collect::<BTreeMap<_, _>>(),
            progress_group: value.progress_group,
        }
    }
}

impl OpMetadata {
    /// Merge metadata from a later vertex that has the same content digest.
    ///
    /// Operation bytes are deduplicated by digest, but BuildKit still merges
    /// metadata from every occurrence of that digest.
    pub(crate) fn merge_from(&mut self, other: &Self) {
        self.ignore_cache |= other.ignore_cache;
        self.description.extend(
            other
                .description
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        if other.export_cache.is_some() {
            self.export_cache = other.export_cache;
        }
        if other.progress_group.is_some() {
            self.progress_group = other.progress_group.clone();
        }
        self.caps.extend(other.caps.iter().cloned());
    }
}
