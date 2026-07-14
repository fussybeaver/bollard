//! Merge-operation builder: combine multiple filesystem states into one.
//!
//! Mirrors Go's `github.com/moby/buildkit/client/llb.Merge`.

use bollard_buildkit_proto::pb;
use prost::Message;

use crate::error::LlbError;
use crate::marshal::{sha256_op, Digest};
use crate::metadata::{attr, cap, OpMetadata};
use crate::ops::{Context, Node, NodeRef, Operation, OperationOutput};
use crate::state::State;

/// Options for a merge operation.
#[derive(Clone, Debug, Default)]
pub struct MergeOpts {
    /// Custom name for the merge vertex.
    custom_name: Option<String>,
}

impl MergeOpts {
    /// Create default merge options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a custom name for the merge vertex.
    pub fn with_custom_name<S: Into<String>>(mut self, name: S) -> Self {
        self.custom_name = Some(name.into());
        self
    }
}

/// Merge multiple states into a single state.
///
/// The resulting filesystem is an overlay of all inputs. A merge with fewer
/// than two non-scratch inputs is collapsed:
/// - zero inputs → [`crate::scratch()`]
/// - one input → the input itself
/// - two or more inputs → a `MergeOp` vertex
pub fn merge<I: IntoIterator<Item = State>>(inputs: I, opts: impl Into<MergeOpts>) -> State {
    let opts = opts.into();
    let inputs: Vec<State> = inputs.into_iter().collect();
    match inputs.len() {
        0 => crate::scratch(),
        1 => inputs.into_iter().next().expect("one input"),
        _ => {
            let op = MergeOp::new(inputs, opts);
            State::new(OperationOutput::Owned(std::sync::Arc::new(op)))
        }
    }
}

/// A fully assembled merge operation.
#[derive(Clone, Debug)]
pub(crate) struct MergeOp {
    inputs: Vec<OperationOutput>,
    bytes: Vec<u8>,
    digest: Digest,
    metadata: OpMetadata,
}

impl MergeOp {
    /// Build a new merge operation.
    ///
    /// Each input gets its own [`pb::Input`] and [`pb::MergeInput`]; there is
    /// no input deduplication, matching Go's `MergeOp.Marshal`.
    pub(crate) fn new(states: Vec<State>, opts: MergeOpts) -> Self {
        let inputs: Vec<OperationOutput> = states.iter().map(|s| s.output().clone()).collect();

        let pb_inputs: Vec<pb::Input> = inputs
            .iter()
            .map(|out| pb::Input {
                digest: out.operation().digest().as_str().to_string(),
                index: out.index().0 as i64,
            })
            .collect();

        let merge_inputs: Vec<pb::MergeInput> = (0..inputs.len())
            .map(|i| pb::MergeInput { input: i as i64 })
            .collect();

        let merge_op = pb::MergeOp {
            inputs: merge_inputs,
        };

        let pb_op = pb::Op {
            inputs: pb_inputs,
            platform: None,
            constraints: None,
            op: Some(pb::op::Op::Merge(merge_op)),
        };

        let digest = sha256_op(&pb_op).expect("MergeOp protobuf encoding is infallible");
        let mut bytes = Vec::new();
        pb_op
            .encode(&mut bytes)
            .expect("MergeOp protobuf encoding is infallible");

        let mut metadata = OpMetadata::default();
        metadata.caps.insert(cap::CAP_MERGE_OP.to_string());
        if let Some(name) = opts.custom_name {
            metadata
                .description
                .insert(attr::DESCRIPTION_NAME.to_string(), name);
            metadata.caps.insert(cap::CAP_META_DESCRIPTION.to_string());
        }

        Self {
            inputs,
            bytes,
            digest,
            metadata,
        }
    }
}

impl Operation for MergeOp {
    fn digest(&self) -> &Digest {
        &self.digest
    }

    fn serialize(&self, ctx: &mut Context) -> Result<NodeRef, LlbError> {
        for input in &self.inputs {
            ctx.register(input)?;
        }
        Ok(ctx.insert_node(Node {
            bytes: self.bytes.clone(),
            digest: self.digest.clone(),
            metadata: self.metadata.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_zero_inputs_is_scratch() {
        let s = merge(Vec::<State>::new(), MergeOpts::new());
        // The scratch source is the canonical empty state.
        assert_eq!(
            s.output().operation().digest().as_str(),
            crate::scratch().output().operation().digest().as_str()
        );
    }

    #[test]
    fn merge_one_input_returns_input() {
        let img = crate::image("alpine:latest");
        let s = merge(vec![img.clone()], MergeOpts::new());
        assert_eq!(
            s.output().operation().digest().as_str(),
            img.output().operation().digest().as_str()
        );
    }

    #[test]
    fn mergeop_digest_stable() {
        let a = merge(
            vec![
                crate::image("alpine:latest"),
                crate::image("busybox:latest"),
            ],
            MergeOpts::new(),
        );
        let b = merge(
            vec![
                crate::image("alpine:latest"),
                crate::image("busybox:latest"),
            ],
            MergeOpts::new(),
        );
        assert_eq!(
            a.output().operation().digest().as_str(),
            b.output().operation().digest().as_str()
        );
    }

    #[test]
    fn mergeop_has_two_inputs() {
        let op = MergeOp::new(
            vec![
                crate::image("alpine:latest"),
                crate::image("busybox:latest"),
            ],
            MergeOpts::new(),
        );
        assert_eq!(op.inputs.len(), 2);
    }

    #[test]
    fn mergeop_variant_is_merge() {
        let op = MergeOp::new(
            vec![
                crate::image("alpine:latest"),
                crate::image("busybox:latest"),
            ],
            MergeOpts::new(),
        );
        let pb_op = pb::Op::decode(op.bytes.as_slice()).unwrap();
        assert!(
            matches!(pb_op.op, Some(pb::op::Op::Merge(_))),
            "expected Merge variant"
        );
    }

    #[test]
    fn mergeop_custom_name_metadata() {
        let op = MergeOp::new(
            vec![crate::image("alpine:latest")],
            MergeOpts::new().with_custom_name("merged"),
        );
        assert_eq!(
            op.metadata
                .description
                .get(crate::metadata::attr::DESCRIPTION_NAME),
            Some(&"merged".to_string())
        );
        assert!(op
            .metadata
            .caps
            .contains(crate::metadata::cap::CAP_META_DESCRIPTION));
    }
}
