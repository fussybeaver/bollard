//! Core operation graph types: [`Operation`], [`Context`], [`Node`], and
//! [`OperationOutput`].

pub(crate) mod exec;
pub(crate) mod file;
pub(crate) mod source;

use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::error::LlbError;
use crate::marshal::Digest;
use crate::metadata::OpMetadata;

/// Index of an output produced by an operation.
///
/// Most operations produce a single output at index 0. Multi-output operations
/// (notably `ExecOp` mounts) use borrowed outputs with non-zero indices.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct OutputIdx(pub u32);

impl OutputIdx {
    /// The primary output of an operation.
    pub const PRIMARY: OutputIdx = OutputIdx(0);
}

/// A handle to an operation output, either owned by a [`State`] or borrowed
/// from a multi-output operation.
#[derive(Clone, Debug)]
pub(crate) enum OperationOutput {
    /// A single-output operation that is the direct producer of a state.
    Owned(Arc<dyn Operation>),
    /// A specific output of a multi-output operation.
    Borrowed {
        /// The operation that owns the output.
        op: Arc<dyn Operation>,
        /// Which output index this handle refers to.
        index: OutputIdx,
    },
}

impl OperationOutput {
    /// The operation that produces this output.
    pub(crate) fn operation(&self) -> &dyn Operation {
        match self {
            OperationOutput::Owned(op) => op.as_ref(),
            OperationOutput::Borrowed { op, .. } => op.as_ref(),
        }
    }

    /// The output index within the operation.
    pub(crate) fn index(&self) -> OutputIdx {
        match self {
            OperationOutput::Owned(_) => OutputIdx::PRIMARY,
            OperationOutput::Borrowed { index, .. } => *index,
        }
    }
}

/// Trait implemented by every LLB operation node.
///
/// Implementations must be cheaply cloneable via an enclosing [`Arc`], implement
/// [`Debug`], and are responsible for recursively registering their inputs with
/// the [`Context`] before serializing themselves.
pub(crate) trait Operation: Send + Sync + Debug {
    /// Content digest of the serialized [`bollard_buildkit_proto::pb::Op`].
    ///
    /// This is the stable identity used for dedup; two operations with the
    /// same digest serialize to identical bytes.
    fn digest(&self) -> &Digest;

    /// Serialize this operation into a [`pb::Op`], register its inputs, and
    /// insert the resulting [`Node`] into `ctx`.
    ///
    /// The returned [`NodeRef`] typically points to the operation's primary
    /// output; callers that need a different output index should use
    /// [`Context::register`].
    fn serialize(&self, ctx: &mut Context) -> Result<NodeRef, LlbError>;

    /// The outputs this operation exposes.
    fn outputs(&self) -> Vec<OutputIdx> {
        vec![OutputIdx::PRIMARY]
    }
}

/// Reference to a registered operation node and a specific output index.
#[derive(Clone, Debug)]
pub(crate) struct NodeRef {
    digest: Digest,
    index: OutputIdx,
}

impl NodeRef {
    /// Build a new node reference.
    pub(crate) fn new(digest: Digest, index: OutputIdx) -> Self {
        Self { digest, index }
    }

    /// Digest of the referenced operation.
    pub(crate) fn digest(&self) -> &Digest {
        &self.digest
    }

    /// Output index within the referenced operation.
    pub(crate) fn index(&self) -> OutputIdx {
        self.index
    }
}

/// A serialized operation vertex, ready to be placed into a [`Definition`].
#[derive(Clone, Debug)]
pub(crate) struct Node {
    /// Prost-encoded [`bollard_buildkit_proto::pb::Op`] bytes.
    pub bytes: Vec<u8>,
    /// Content digest of `bytes`.
    pub digest: Digest,
    /// Per-vertex metadata.
    pub metadata: OpMetadata,
}

/// Marshaling context: maintains the post-order register table and dedups
/// operations by content digest.
#[derive(Clone, Debug, Default)]
pub(crate) struct Context {
    /// Registered nodes in insertion (post-order) order.
    nodes: IndexMap<Digest, Node>,
    /// Digests that have already been serialized.
    seen: HashSet<Digest>,
}

impl Context {
    /// Create an empty context.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Ensure an output's producing operation is serialized, returning a
    /// reference to it.
    ///
    /// On first encounter the operation is serialized, which recursively
    /// registers its inputs first. Subsequent references short-circuit.
    pub(crate) fn register(&mut self, output: &OperationOutput) -> Result<NodeRef, LlbError> {
        let op = output.operation();
        let digest = op.digest().clone();
        if !self.seen.contains(&digest) {
            op.serialize(self)?;
            self.seen.insert(digest.clone());
        }
        Ok(NodeRef::new(digest, output.index()))
    }

    /// Insert a serialized node into the context.
    ///
    /// Callers (operation `serialize` implementations) are responsible for
    /// having already registered all inputs.
    pub(crate) fn insert_node(&mut self, node: Node) -> NodeRef {
        let index = OutputIdx::PRIMARY;
        let digest = node.digest.clone();
        debug_assert!(
            !self.nodes.contains_key(&digest),
            "duplicate node insertion for {digest}"
        );
        self.nodes.insert(digest.clone(), node);
        NodeRef::new(digest, index)
    }

    /// Iterate registered nodes in post-order (children before parents).
    pub(crate) fn nodes(&self) -> &IndexMap<Digest, Node> {
        &self.nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_starts_empty() {
        let ctx = Context::new();
        assert!(ctx.nodes().is_empty());
    }
}
