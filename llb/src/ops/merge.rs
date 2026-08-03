//! Merge-operation builder: combine multiple filesystem states into one.
//!
//! Mirrors Go's `github.com/moby/buildkit/client/llb.Merge`.

use bollard_buildkit_proto::pb;

use crate::error::LlbError;
use crate::marshal::encode_and_hash;
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
/// The resulting filesystem is an overlay of all non-scratch inputs. Scratch
/// (empty) inputs are filtered out, matching Go's `llb.Merge` behavior. A
/// merge with fewer than two non-scratch inputs is collapsed:
/// - zero non-scratch inputs → [`crate::scratch()`]
/// - one non-scratch input → the input itself
/// - two or more non-scratch inputs → a `MergeOp` vertex
pub fn merge<I: IntoIterator<Item = State>>(
    inputs: I,
    opts: impl Into<MergeOpts>,
) -> Result<State, LlbError> {
    let opts = opts.into();
    let inputs: Vec<State> = inputs
        .into_iter()
        .filter(|s| !s.output().is_empty())
        .collect();
    match inputs.len() {
        0 => crate::scratch(),
        1 => Ok(inputs.into_iter().next().expect("one input")),
        _ => {
            let constraints = inputs
                .first()
                .map(|state| state.constraints().clone())
                .unwrap_or_default();
            let op = MergeOp::new(inputs, opts)?;
            Ok(State::with_constraints(
                OperationOutput::Owned(std::sync::Arc::new(op)),
                constraints,
            ))
        }
    }
}

/// A fully assembled merge operation.
#[derive(Clone, Debug)]
pub(crate) struct MergeOp {
    inputs: Vec<OperationOutput>,
    metadata: OpMetadata,
}

impl MergeOp {
    /// Build a new merge operation.
    ///
    /// Each input gets its own [`pb::Input`] and [`pb::MergeInput`]; there is
    /// no input deduplication, matching Go's `MergeOp.Marshal`. The actual
    /// protobuf bytes are computed at marshal time so that the active worker
    /// constraints affect the content digest.
    pub(crate) fn new(states: Vec<State>, opts: MergeOpts) -> Result<Self, LlbError> {
        let inputs: Vec<OperationOutput> = states.iter().map(|s| s.output().clone()).collect();

        let mut metadata = OpMetadata::default();
        metadata.caps.insert(cap::CAP_MERGE_OP.to_string());
        if let Some(name) = opts.custom_name {
            metadata
                .description
                .insert(attr::DESCRIPTION_NAME.to_string(), name);
        }

        Ok(Self { inputs, metadata })
    }
}

impl Operation for MergeOp {
    fn serialize(&self, ctx: &mut Context) -> Result<NodeRef, LlbError> {
        let mut pb_inputs: Vec<pb::Input> = Vec::with_capacity(self.inputs.len());
        for input in &self.inputs {
            if input.is_empty() {
                pb_inputs.push(pb::Input {
                    digest: String::new(),
                    index: -1,
                });
            } else {
                let node_ref = ctx.register(input)?;
                pb_inputs.push(pb::Input {
                    digest: node_ref.digest().as_str().to_string(),
                    index: node_ref.index().0 as i64,
                });
            }
        }

        let merge_inputs: Vec<pb::MergeInput> = (0..self.inputs.len())
            .map(|i| pb::MergeInput { input: i as i64 })
            .collect();

        let merge_op = pb::MergeOp {
            inputs: merge_inputs,
        };

        let pb_op = pb::Op {
            inputs: pb_inputs,
            platform: None,
            constraints: Some(pb::WorkerConstraints {
                filter: ctx.worker_filters().to_vec(),
            }),
            op: Some(pb::op::Op::Merge(merge_op)),
        };

        let (digest, bytes) = encode_and_hash(&pb_op)?;
        Ok(ctx.insert_node(Node {
            bytes,
            digest,
            metadata: self.metadata.clone(),
        }))
    }
}
