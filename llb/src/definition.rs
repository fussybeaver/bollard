//! The marshalled LLB graph.
//!
//! This module is a placeholder for Phase 4. It currently exposes the public
//! `Definition` type so that `State::marshal` can declare its return type.

/// A marshalled LLB graph, ready to be converted into a protobuf
/// [`bollard_buildkit_proto::pb::Definition`].
#[derive(Clone, Debug)]
pub struct Definition {
    _priv: (),
}
