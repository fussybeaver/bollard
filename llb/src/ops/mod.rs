//! Core operation graph types: [`Operation`], [`Context`], [`Node`], and
//! [`OperationOutput`].

pub(crate) mod exec;
pub(crate) mod file;
pub(crate) mod merge;
pub(crate) mod source;

use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::Arc;

use bollard_buildkit_proto::pb;
use indexmap::IndexMap;
use prost::Message;

use crate::definition::Definition;
use crate::error::LlbError;
use crate::marshal::{sha256_op, Digest};
use crate::metadata::{cap, OpMetadata};

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

    /// Append the synthetic root wrapper vertex.
    ///
    /// The wrapper is a no-variant [`pb::Op`] with a single input pointing at
    /// the real root. It exists to anchor the marshal-time constraints (via
    /// capability caps) without mutating the shared, deduplicated real root op.
    /// This matches Go's `client/llb.State.Marshal` output.
    pub(crate) fn append_wrapper(&mut self, root: NodeRef) -> Result<NodeRef, LlbError> {
        let wrapper_op = pb::Op {
            inputs: vec![pb::Input {
                digest: root.digest().as_str().to_string(),
                index: root.index().0 as i64,
            }],
            platform: None,
            constraints: None,
            op: None,
        };

        let digest = sha256_op(&wrapper_op)?;
        let mut bytes = Vec::new();
        wrapper_op
            .encode(&mut bytes)
            .map_err(|source| LlbError::Encode {
                op: "wrapper".to_string(),
                source,
            })?;

        let mut metadata = OpMetadata::default();
        metadata.caps.insert(cap::CAP_CONSTRAINTS.to_string());
        metadata.caps.insert(cap::CAP_PLATFORM.to_string());

        for node in self.nodes.values() {
            if node.metadata.ignore_cache {
                metadata.caps.insert(cap::CAP_META_IGNORE_CACHE.to_string());
            }
            if !node.metadata.description.is_empty() {
                metadata.caps.insert(cap::CAP_META_DESCRIPTION.to_string());
            }
            if node.metadata.export_cache.is_some() {
                metadata.caps.insert(cap::CAP_META_EXPORT_CACHE.to_string());
            }
        }

        self.insert_node(Node {
            bytes,
            digest: digest.clone(),
            metadata,
        });
        Ok(NodeRef::new(digest, OutputIdx::PRIMARY))
    }

    /// Finalize the context into a [`Definition`].
    ///
    /// `root` is the digest of the wrapper vertex produced by [`append_wrapper`].
    pub(crate) fn finalize(self, root: Digest) -> Definition {
        let def = self.nodes.values().map(|n| n.bytes.clone()).collect();
        let metadata = self
            .nodes
            .into_iter()
            .map(|(digest, node)| (digest.as_str().to_string(), node.metadata.into()))
            .collect();
        Definition {
            def,
            metadata,
            source: None,
            root,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use bollard_buildkit_proto::pb;
    use prost::Message;

    use super::*;
    use crate::image;
    use crate::marshal::sha256;
    use crate::merge;
    use crate::metadata::cap;
    use crate::scratch;
    use crate::shlex;
    use crate::state::MarshalOpts;

    #[test]
    fn context_starts_empty() {
        let ctx = Context::new();
        assert!(ctx.nodes().is_empty());
    }

    #[test]
    fn marshal_dedup_identical_images() {
        let root = image("alpine:latest");
        let def = root.marshal(MarshalOpts::default()).unwrap();
        // Source op + wrapper = 2 entries.
        assert_eq!(def.def.len(), 2);

        let dup = merge(
            vec![image("alpine:latest"), image("alpine:latest")],
            crate::ops::merge::MergeOpts::new(),
        );
        let def = dup.marshal(MarshalOpts::default()).unwrap();
        // Identical images dedup to one source op, plus merge op, plus wrapper = 3 entries.
        assert_eq!(def.def.len(), 3);
    }

    #[test]
    fn marshal_topological_order() {
        let s = image("alpine:latest").run(shlex("echo hello")).root();
        let def = s.marshal(MarshalOpts::default()).unwrap();

        let mut seen: HashSet<String> = HashSet::new();
        for bytes in &def.def {
            let op = pb::Op::decode(bytes.as_slice()).unwrap();
            for input in &op.inputs {
                assert!(
                    seen.contains(&input.digest),
                    "input {} referenced before it was defined",
                    input.digest
                );
            }
            let dgst = sha256(bytes).to_string();
            seen.insert(dgst);
        }
    }

    #[test]
    fn marshal_wrapper_is_no_variant() {
        let s = scratch();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        let wrapper_bytes = def.def.last().expect("definition is non-empty");
        let wrapper_op = pb::Op::decode(wrapper_bytes.as_slice()).unwrap();
        assert!(wrapper_op.op.is_none());
        assert_eq!(wrapper_op.inputs.len(), 1);
        assert_eq!(wrapper_op.platform, None);
        assert_eq!(wrapper_op.constraints, None);

        let wrapper_md = def
            .metadata
            .get(def.root.as_str())
            .expect("root has metadata");
        assert!(wrapper_md.caps.contains_key(cap::CAP_CONSTRAINTS));
        assert!(wrapper_md.caps.contains_key(cap::CAP_PLATFORM));
    }

    #[test]
    fn marshal_round_trip_stable() {
        let s = image("alpine:latest").run(shlex("echo hello")).root();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        let bytes_a = def.into_bytes().unwrap();

        let pb_def = pb::Definition::decode(bytes_a.as_slice()).unwrap();
        let mut bytes_b = Vec::new();
        pb_def.encode(&mut bytes_b).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }

    #[test]
    fn marshal_full_chain() {
        let s = image("alpine:latest").run(shlex("echo hello")).root();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        assert!(!def.def.is_empty());

        let last_dgst = sha256(def.def.last().unwrap());
        assert_eq!(def.root.as_str(), last_dgst.as_str());
    }

    #[test]
    fn marshal_propagates_meta_caps_to_wrapper() {
        let s = image("alpine:latest")
            .run(shlex("echo hello"))
            .with_custom_name("hello")
            .root();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        let wrapper_md = def
            .metadata
            .get(def.root.as_str())
            .expect("root has metadata");
        assert!(wrapper_md.caps.contains_key(cap::CAP_META_DESCRIPTION));
    }
}
