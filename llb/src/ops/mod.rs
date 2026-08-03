//! Core operation graph types: [`Operation`], [`Context`], [`Node`], and
//! [`OperationOutput`].

pub(crate) mod exec;
pub(crate) mod file;
pub(crate) mod merge;
pub(crate) mod source;

use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::sync::Arc;

use bollard_buildkit_proto::pb;
use indexmap::IndexMap;

use crate::definition::Definition;
use crate::error::LlbError;
use crate::marshal::{encode_and_hash, Digest};
use crate::metadata::{attr, cap, OpMetadata};
use crate::platform::Platform;

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
    /// An empty output representing an absent (scratch) filesystem.
    ///
    /// Go's `llb.Scratch()` does not produce a vertex; it is encoded as an
    /// input index of `-1` on the consuming operation.
    Empty,
}

impl OperationOutput {
    /// The operation that produces this output.
    ///
    /// # Panics
    ///
    /// Panics if called on [`OperationOutput::Empty`].
    pub(crate) fn operation(&self) -> &dyn Operation {
        match self {
            OperationOutput::Owned(op) => op.as_ref(),
            OperationOutput::Borrowed { op, .. } => op.as_ref(),
            OperationOutput::Empty => panic!("empty output has no operation"),
        }
    }

    /// The output index within the operation.
    pub(crate) fn index(&self) -> OutputIdx {
        match self {
            OperationOutput::Owned(_) => OutputIdx::PRIMARY,
            OperationOutput::Borrowed { index, .. } => *index,
            OperationOutput::Empty => OutputIdx::PRIMARY,
        }
    }

    /// Returns `true` if this output represents an empty scratch filesystem.
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, OperationOutput::Empty)
    }
}

/// Trait implemented by every LLB operation node.
///
/// Implementations must be cheaply cloneable via an enclosing [`Arc`], implement
/// [`Debug`], and are responsible for recursively registering their inputs with
/// the [`Context`] before serializing themselves.
pub(crate) trait Operation: Send + Sync + Debug {
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

/// Marshaling context: maintains the post-order register table, dedups
/// operations by content digest, and holds the active marshal-time constraints.
#[derive(Clone, Debug)]
pub(crate) struct Context {
    /// Registered nodes in insertion (post-order) order.
    nodes: IndexMap<Digest, Node>,
    /// Cache of serialized operation identities to their node digest.
    ///
    /// This avoids re-traversing shared subgraphs while still computing the
    /// final content digest from the encoded bytes.
    serialized: HashMap<(usize, OutputIdx), NodeRef>,
    /// Marshal-time platform constraint.
    platform: Option<Platform>,
    /// Marshal-time worker constraint filters.
    worker_filters: Vec<String>,
}

impl Context {
    /// Create a context with the given marshal-time constraints.
    pub(crate) fn new(platform: Option<Platform>, worker_filters: Vec<String>) -> Self {
        Self {
            nodes: IndexMap::new(),
            serialized: HashMap::new(),
            platform,
            worker_filters,
        }
    }

    /// Ensure an output's producing operation is serialized, returning a
    /// reference to it.
    ///
    /// On first encounter the operation is serialized, which recursively
    /// registers its inputs first. Subsequent references to the same
    /// operation object short-circuit through an identity cache.
    pub(crate) fn register(&mut self, output: &OperationOutput) -> Result<NodeRef, LlbError> {
        if output.is_empty() {
            return Ok(NodeRef::new(Digest::empty(), OutputIdx::PRIMARY));
        }
        let op = output.operation();
        let key = (operation_identity_key(op), output.index());
        if let Some(node_ref) = self.serialized.get(&key) {
            return Ok(node_ref.clone());
        }
        let node_ref = op.serialize(self)?;
        self.serialized.insert(key, node_ref.clone());
        Ok(node_ref)
    }

    /// Insert a serialized node into the context.
    ///
    /// Callers (operation `serialize` implementations) are responsible for
    /// having already registered all inputs.
    ///
    /// If a node with the same content digest is already present, the existing
    /// entry is reused for deduplication.
    pub(crate) fn insert_node(&mut self, node: Node) -> NodeRef {
        let digest = node.digest.clone();
        self.nodes.entry(digest.clone()).or_insert(node);
        NodeRef::new(digest, OutputIdx::PRIMARY)
    }

    /// Iterate registered nodes in post-order (children before parents).
    pub(crate) fn nodes(&self) -> &IndexMap<Digest, Node> {
        &self.nodes
    }

    /// Return the active marshal-time platform constraint.
    pub(crate) fn platform(&self) -> Option<Platform> {
        self.platform.clone()
    }

    /// Return the active marshal-time worker filters.
    pub(crate) fn worker_filters(&self) -> &[String] {
        &self.worker_filters
    }

    /// Combine the active marshal-time platform with an operation's own
    /// platform. Operation-local constraints override the marshal default.
    pub(crate) fn combined_platform(
        &self,
        operation_platform: Option<Platform>,
    ) -> Option<Platform> {
        operation_platform.or_else(|| self.platform.clone())
    }

    /// Append the synthetic root wrapper vertex.
    ///
    /// The wrapper is a no-variant [`pb::Op`] with a single input pointing at
    /// the real root. It does not carry platform or worker constraints; those
    /// are now placed on each real operation. The wrapper metadata still
    /// advertises the scheduling capabilities used by the graph.
    pub(crate) fn append_wrapper(
        &mut self,
        root: NodeRef,
        custom_name: Option<&str>,
    ) -> Result<NodeRef, LlbError> {
        let wrapper_op = pb::Op {
            inputs: vec![pb::Input {
                digest: root.digest().as_str().to_string(),
                index: root.index().0 as i64,
            }],
            platform: None,
            constraints: None,
            op: None,
        };

        let (digest, bytes) = encode_and_hash(&wrapper_op)?;

        let mut metadata = OpMetadata::default();
        metadata.caps.insert(cap::CAP_CONSTRAINTS.to_string());
        metadata.caps.insert(cap::CAP_PLATFORM.to_string());

        if let Some(name) = custom_name {
            metadata
                .description
                .insert(attr::DESCRIPTION_NAME.to_string(), name.to_string());
            metadata.caps.insert(cap::CAP_META_DESCRIPTION.to_string());
        }

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
    /// `wrapper_root` is the digest of the wrapper vertex produced by
    /// [`append_wrapper`]. `head` is the real graph head referenced by it.
    pub(crate) fn finalize(self, wrapper_root: Digest, head: Option<Digest>) -> Definition {
        let mut locations = BTreeMap::new();
        let mut def = Vec::with_capacity(self.nodes.len());
        let mut metadata = BTreeMap::new();
        for (digest, node) in self.nodes {
            if digest != wrapper_root {
                // Go's source-map collector records every real vertex, even
                // when it has no user-facing source locations. Keep the
                // entries empty so the direct-solve boundary can remove them
                // for older BuildKit daemons when necessary.
                locations.insert(
                    digest.as_str().to_string(),
                    pb::Locations {
                        locations: Vec::new(),
                    },
                );
            }
            def.push(node.bytes);
            metadata.insert(digest.as_str().to_string(), node.metadata.into());
        }
        Definition {
            def,
            metadata,
            source: Some(pb::Source {
                locations,
                infos: Vec::new(),
            }),
            root: head,
        }
    }
}

/// Return a stable identity key for an operation object.
///
/// The key is the address of the [`Arc`]-allocated operation, used to avoid
/// re-serializing the same operation during a single marshal pass.
fn operation_identity_key(op: &dyn Operation) -> usize {
    let ptr: *const dyn Operation = op;
    ptr as *const () as usize
}
