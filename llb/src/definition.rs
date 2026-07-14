//! The marshalled LLB graph.
//!
//! A [`Definition`] is the end result of [`State::marshal`](crate::state::State::marshal).
//! It can be converted into a protobuf [`bollard_buildkit_proto::pb::Definition`]
//! and then encoded to bytes for a BuildKit `SolveRequest`.

use std::collections::BTreeMap;

use bollard_buildkit_proto::pb;
use prost::Message;

use crate::error::LlbError;
use crate::marshal::Digest;

/// A marshalled LLB graph.
///
/// The `def` field is a topologically sorted list of serialized [`pb::Op`]
/// vertices: children precede parents, and the final entry is a synthetic
/// wrapper vertex that points at the real root.
#[derive(Clone, Debug)]
pub struct Definition {
    /// Serialized [`pb::Op`] bytes in topological (post-order) order.
    pub def: Vec<Vec<u8>>,

    /// Per-vertex metadata keyed by op digest string.
    pub metadata: BTreeMap<String, pb::OpMetadata>,

    /// Optional source mapping information. Currently unused in this crate.
    pub source: Option<pb::Source>,

    /// Digest of the root (last) vertex.
    pub root: Digest,
}

impl Definition {
    /// Convert this definition into a protobuf [`pb::Definition`].
    pub fn to_pb(&self) -> pb::Definition {
        pb::Definition {
            def: self.def.clone(),
            metadata: self.metadata.clone(),
            source: self.source.clone(),
        }
    }

    /// Encode this definition as protobuf bytes.
    pub fn into_bytes(self) -> Result<Vec<u8>, LlbError> {
        let mut buf = Vec::new();
        self.to_pb()
            .encode(&mut buf)
            .map_err(|source| LlbError::Encode {
                op: "Definition".to_string(),
                source,
            })?;
        Ok(buf)
    }
}
