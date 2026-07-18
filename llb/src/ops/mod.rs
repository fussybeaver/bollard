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
    use crate::platform::Platform;
    use crate::scratch;
    use crate::shlex;
    use crate::state::MarshalOpts;

    #[test]
    fn context_starts_empty() {
        let ctx = Context::new(None, Vec::new());
        assert!(ctx.nodes().is_empty());
    }

    #[test]
    fn marshal_scratch_direct_is_empty() {
        let def = scratch().unwrap().marshal(MarshalOpts::default()).unwrap();
        assert!(def.def.is_empty());
        assert!(def.metadata.is_empty());
        assert!(def.source.is_none());
        assert_eq!(def.root, None);
    }

    #[test]
    fn marshal_source_map_covers_real_vertices_not_wrapper() {
        let state = image("alpine:latest")
            .unwrap()
            .run(shlex("echo hello"))
            .root()
            .unwrap();
        let def = state.marshal(MarshalOpts::default()).unwrap();
        let source = def
            .source
            .as_ref()
            .expect("non-empty definitions have source maps");

        assert!(source.infos.is_empty());
        assert_eq!(source.locations.len(), def.def.len() - 1);
        for bytes in def.def.iter().take(def.def.len() - 1) {
            assert!(source.locations.contains_key(sha256(bytes).as_str()));
        }
        let head = def.root.as_ref().expect("non-empty definition has a head");
        assert!(source.locations.contains_key(head.as_str()));
        let wrapper_digest = sha256(def.def.last().expect("definition has wrapper"));
        assert!(!source.locations.contains_key(wrapper_digest.as_str()));
    }

    #[test]
    fn marshal_dedup_identical_images() {
        let root = image("alpine:latest").unwrap();
        let def = root.marshal(MarshalOpts::default()).unwrap();
        // Source op + wrapper = 2 entries.
        assert_eq!(def.def.len(), 2);

        let dup = merge(
            vec![
                image("alpine:latest").unwrap(),
                image("alpine:latest").unwrap(),
            ],
            crate::ops::merge::MergeOpts::new(),
        )
        .unwrap();
        let def = dup.marshal(MarshalOpts::default()).unwrap();
        // Identical images dedup to one source op, plus merge op, plus wrapper = 3 entries.
        assert_eq!(def.def.len(), 3);
    }

    #[test]
    fn marshal_dedup_identical_exec_ops() {
        let base = image("alpine:latest").unwrap();
        let a = base.clone().run(shlex("echo hello")).root().unwrap();
        let b = base.run(shlex("echo hello")).root().unwrap();
        let def = merge(vec![a, b], crate::ops::merge::MergeOpts::new())
            .unwrap()
            .marshal(MarshalOpts::default())
            .unwrap();
        // One image + one exec (deduped) + merge + wrapper = 4 entries.
        assert_eq!(def.def.len(), 4);
    }

    #[test]
    fn marshal_dedup_identical_file_ops() {
        let base = scratch().unwrap();
        let a = base
            .clone()
            .file(
                crate::mkdir("/tmp", 0o755).with_parents(true),
                crate::FileOpts::new(),
            )
            .unwrap();
        let b = base
            .file(
                crate::mkdir("/tmp", 0o755).with_parents(true),
                crate::FileOpts::new(),
            )
            .unwrap();
        let def = merge(vec![a, b], crate::ops::merge::MergeOpts::new())
            .unwrap()
            .marshal(MarshalOpts::default())
            .unwrap();
        // Scratch produces no vertex. One mkdir (deduped) + merge + wrapper = 3 entries.
        assert_eq!(def.def.len(), 3);
    }

    #[test]
    fn marshal_dedup_shared_subgraph() {
        let alpine = image("alpine:latest").unwrap();
        let branch_a = alpine.clone().run(shlex("echo a")).root().unwrap();
        let branch_b = alpine.run(shlex("echo b")).root().unwrap();
        let def = merge(
            vec![branch_a, branch_b],
            crate::ops::merge::MergeOpts::new(),
        )
        .unwrap()
        .marshal(MarshalOpts::default())
        .unwrap();
        // One shared image + two distinct execs + merge + wrapper = 5 entries.
        assert_eq!(def.def.len(), 5);
    }

    #[test]
    fn marshal_dedup_count_exact() {
        let def = merge(
            vec![
                image("alpine:latest").unwrap(),
                image("busybox:latest").unwrap(),
                image("alpine:latest").unwrap(),
            ],
            crate::ops::merge::MergeOpts::new(),
        )
        .unwrap()
        .marshal(MarshalOpts::default())
        .unwrap();
        // Two unique images (alpine deduped) + merge + wrapper = 4 entries.
        assert_eq!(def.def.len(), 4);
    }

    #[test]
    fn marshal_topological_order() {
        let s = image("alpine:latest")
            .unwrap()
            .run(shlex("echo hello"))
            .root()
            .unwrap();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        assert_topological_order(&def);
    }

    #[test]
    fn marshal_topological_order_deep_chain() {
        let s = image("alpine:latest")
            .unwrap()
            .run(shlex("echo 1"))
            .root()
            .unwrap()
            .run(shlex("echo 2"))
            .root()
            .unwrap()
            .run(shlex("echo 3"))
            .root()
            .unwrap()
            .run(shlex("echo 4"))
            .root()
            .unwrap()
            .run(shlex("echo 5"))
            .root()
            .unwrap();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        assert_topological_order(&def);
    }

    #[test]
    fn marshal_topological_order_diamond() {
        let alpine = image("alpine:latest").unwrap();
        let branch_a = alpine.clone().run(shlex("echo a")).root().unwrap();
        let branch_b = alpine.run(shlex("echo b")).root().unwrap();
        let s = merge(
            vec![branch_a, branch_b],
            crate::ops::merge::MergeOpts::new(),
        )
        .unwrap();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        assert_topological_order(&def);
    }

    #[test]
    fn marshal_topological_order_merge_with_file_ops() {
        let image_branch = image("alpine:latest")
            .unwrap()
            .file(
                crate::mkdir("/app", 0o755).with_parents(true),
                crate::FileOpts::new(),
            )
            .unwrap()
            .run(shlex("echo hello"))
            .root()
            .unwrap();
        let scratch_branch = scratch()
            .unwrap()
            .file(crate::mkfile("/tmp", 0o644, b"x"), crate::FileOpts::new())
            .unwrap();
        let s = merge(
            vec![image_branch, scratch_branch],
            crate::ops::merge::MergeOpts::new(),
        )
        .unwrap();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        assert_topological_order(&def);
    }

    fn assert_topological_order(def: &Definition) {
        let mut seen: HashSet<String> = HashSet::new();
        for bytes in &def.def {
            let op = pb::Op::decode(bytes.as_slice()).unwrap();
            for input in &op.inputs {
                if input.index < 0 {
                    // Scratch inputs have no digest.
                    continue;
                }
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

    fn find_op_by_variant<F>(def: &Definition, mut predicate: F) -> Option<pb::Op>
    where
        F: FnMut(&pb::op::Op) -> bool,
    {
        def.def
            .iter()
            .map(|bytes| pb::Op::decode(bytes.as_slice()).unwrap())
            .find(|op| op.op.as_ref().is_some_and(&mut predicate))
    }

    #[test]
    fn marshal_wrapper_is_no_variant() {
        let s = image("alpine:latest").unwrap();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        let wrapper_bytes = def.def.last().expect("definition is non-empty");
        let wrapper_op = pb::Op::decode(wrapper_bytes.as_slice()).unwrap();
        assert!(wrapper_op.op.is_none());
        assert_eq!(wrapper_op.inputs.len(), 1);
        assert_eq!(wrapper_op.platform, None);
        assert_eq!(wrapper_op.constraints, None);

        let wrapper_md = def
            .metadata
            .get(sha256(def.def.last().expect("definition has wrapper")).as_str())
            .expect("root has metadata");
        assert!(wrapper_md.caps.contains_key(cap::CAP_CONSTRAINTS));
        assert!(wrapper_md.caps.contains_key(cap::CAP_PLATFORM));
    }

    #[test]
    fn marshal_round_trip_stable() {
        let s = image("alpine:latest")
            .unwrap()
            .run(shlex("echo hello"))
            .root()
            .unwrap();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        let bytes_a = def.into_bytes().unwrap();

        let pb_def = pb::Definition::decode(bytes_a.as_slice()).unwrap();
        let mut bytes_b = Vec::new();
        pb_def.encode(&mut bytes_b).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }

    #[test]
    fn marshal_round_trip_multi_step() {
        let s = image("alpine:latest")
            .unwrap()
            .run(shlex("echo one"))
            .root()
            .unwrap()
            .run(shlex("echo two"))
            .root()
            .unwrap()
            .run(shlex("echo three"))
            .root()
            .unwrap();
        let def = s.marshal(MarshalOpts::linux_amd64()).unwrap();
        let bytes_a = def.into_bytes().unwrap();

        let pb_def = pb::Definition::decode(bytes_a.as_slice()).unwrap();
        let mut bytes_b = Vec::new();
        pb_def.encode(&mut bytes_b).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }

    #[test]
    fn marshal_round_trip_merge_exec() {
        let merged = merge(
            vec![
                image("alpine:latest").unwrap(),
                image("busybox:latest").unwrap(),
            ],
            crate::ops::merge::MergeOpts::new(),
        )
        .unwrap();
        let s = merged.run(shlex("echo hello")).root().unwrap();
        let def = s.marshal(MarshalOpts::linux_amd64()).unwrap();
        let bytes_a = def.into_bytes().unwrap();

        let pb_def = pb::Definition::decode(bytes_a.as_slice()).unwrap();
        let mut bytes_b = Vec::new();
        pb_def.encode(&mut bytes_b).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }

    #[test]
    fn marshal_round_trip_file_chain() {
        use crate::{mkdir, mkfile};
        let s = scratch()
            .unwrap()
            .file(
                mkdir("/app", 0o755).with_parents(true),
                crate::FileOpts::new(),
            )
            .unwrap()
            .file(
                mkfile("/app/hello", 0o644, b"world"),
                crate::FileOpts::new(),
            )
            .unwrap();
        let def = s.marshal(MarshalOpts::linux_amd64()).unwrap();
        let bytes_a = def.into_bytes().unwrap();

        let pb_def = pb::Definition::decode(bytes_a.as_slice()).unwrap();
        let mut bytes_b = Vec::new();
        pb_def.encode(&mut bytes_b).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }

    #[test]
    fn marshal_full_chain() {
        let s = image("alpine:latest")
            .unwrap()
            .run(shlex("echo hello"))
            .root()
            .unwrap();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        assert!(!def.def.is_empty());

        let wrapper = pb::Op::decode(def.def.last().unwrap().as_slice()).unwrap();
        let head = def.root.as_ref().expect("non-empty definition has a head");
        assert_eq!(wrapper.inputs[0].digest, head.as_str());
        assert_ne!(head.as_str(), sha256(def.def.last().unwrap()).as_str());
    }

    #[test]
    fn marshal_propagates_meta_caps_to_wrapper() {
        let s = image("alpine:latest")
            .unwrap()
            .run(shlex("echo hello"))
            .with_custom_name("hello")
            .root()
            .unwrap();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        let wrapper_md = def
            .metadata
            .get(sha256(def.def.last().expect("definition has wrapper")).as_str())
            .expect("root has metadata");
        assert!(wrapper_md.caps.contains_key(cap::CAP_META_DESCRIPTION));
    }

    #[test]
    fn marshal_image_op_platform_default_is_linux_amd64() {
        let s = image("alpine:latest").unwrap();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        let image_op = find_op_by_variant(&def, |op| matches!(op, pb::op::Op::Source(_)))
            .expect("expected image source op");
        assert_eq!(
            image_op.platform,
            Some(pb::Platform {
                architecture: "amd64".to_string(),
                os: "linux".to_string(),
                variant: String::new(),
                os_version: String::new(),
                os_features: Vec::new(),
            })
        );
        assert!(image_op.constraints.is_some());
    }

    #[test]
    fn marshal_image_op_platform_none_when_opts_explicitly_none() {
        let s = image("alpine:latest").unwrap();
        let def = s
            .marshal(MarshalOpts {
                platform: None,
                worker_filters: Vec::new(),
            })
            .unwrap();
        let image_op = find_op_by_variant(&def, |op| matches!(op, pb::op::Op::Source(_)))
            .expect("expected image source op");
        assert_eq!(image_op.platform, None);
    }

    #[test]
    fn marshal_image_op_platform_linux_amd64() {
        let s = image("alpine:latest").unwrap();
        let def = s.marshal(MarshalOpts::linux_amd64()).unwrap();
        let image_op = find_op_by_variant(&def, |op| matches!(op, pb::op::Op::Source(_)))
            .expect("expected image source op");
        assert_eq!(
            image_op.platform,
            Some(pb::Platform {
                architecture: "amd64".to_string(),
                os: "linux".to_string(),
                variant: String::new(),
                os_version: String::new(),
                os_features: Vec::new(),
            })
        );
    }

    #[test]
    fn marshal_definition_digest_differs_with_platform() {
        let s = image("alpine:latest").unwrap();
        let def_default = s.marshal(MarshalOpts::default()).unwrap();
        let def_arm64 = s
            .marshal(MarshalOpts::default().with_platform(Platform::LINUX_ARM64.clone()))
            .unwrap();
        assert_ne!(def_default.root, def_arm64.root);
    }

    #[test]
    fn marshal_state_custom_name_in_wrapper_metadata() {
        let s = image("alpine:latest")
            .unwrap()
            .with_custom_name("root wrapper name");
        let def = s.marshal(MarshalOpts::default()).unwrap();
        let wrapper_md = def
            .metadata
            .get(sha256(def.def.last().expect("definition has wrapper")).as_str())
            .expect("root has metadata");
        assert_eq!(
            wrapper_md.description.get(attr::DESCRIPTION_NAME),
            Some(&"root wrapper name".to_string())
        );
        assert!(wrapper_md.caps.contains_key(cap::CAP_META_DESCRIPTION));
    }

    #[test]
    fn marshal_state_platform_applies_to_subsequent_exec() {
        let s = image("alpine:latest")
            .unwrap()
            .with_platform(Platform::LINUX_ARM64.clone());
        let def = s
            .run(shlex("echo hello"))
            .root()
            .unwrap()
            .marshal(MarshalOpts {
                platform: None,
                worker_filters: Vec::new(),
            })
            .unwrap();
        let image_op = find_op_by_variant(&def, |op| matches!(op, pb::op::Op::Source(_)))
            .expect("expected image source op");
        assert_eq!(image_op.platform, None);
        let exec_op = find_op_by_variant(&def, |op| matches!(op, pb::op::Op::Exec(_)))
            .expect("expected exec op");
        assert_eq!(exec_op.platform, Some(Platform::LINUX_ARM64.clone().into()));
    }

    #[test]
    fn marshal_state_platform_overrides_marshal_platform() {
        let s = image("alpine:latest")
            .unwrap()
            .with_platform(Platform::LINUX_ARM64.clone());
        let def = s.marshal(MarshalOpts::linux_amd64()).unwrap();
        let image_op = find_op_by_variant(&def, |op| matches!(op, pb::op::Op::Source(_)))
            .expect("expected image source op");
        assert_eq!(
            image_op.platform,
            Some(pb::Platform {
                architecture: "amd64".to_string(),
                os: "linux".to_string(),
                variant: String::new(),
                os_version: String::new(),
                os_features: Vec::new(),
            })
        );
        let exec_op = s
            .run(shlex("echo hello"))
            .root()
            .unwrap()
            .marshal(MarshalOpts::linux_amd64())
            .unwrap();
        let exec_op = find_op_by_variant(&exec_op, |op| matches!(op, pb::op::Op::Exec(_)))
            .expect("expected exec op");
        assert_eq!(exec_op.platform, Some(Platform::LINUX_ARM64.clone().into()));
    }

    #[test]
    fn marshal_worker_constraints_on_real_ops() {
        let s = image("alpine:latest")
            .unwrap()
            .run(shlex("echo hello"))
            .root()
            .unwrap();
        let def = s
            .marshal(MarshalOpts::default().with_worker_filter("foo"))
            .unwrap();
        for bytes in &def.def {
            let op = pb::Op::decode(bytes.as_slice()).unwrap();
            if op.op.is_some() {
                let constraints = op
                    .constraints
                    .as_ref()
                    .expect("real operation should have worker constraints");
                assert_eq!(constraints.filter, vec!["foo"]);
            } else {
                assert!(
                    op.constraints.is_none(),
                    "wrapper should have no constraints"
                );
            }
        }
    }
}
