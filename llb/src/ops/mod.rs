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

use crate::definition::Definition;
use crate::error::LlbError;
use crate::marshal::{encode_and_hash, Digest};
use crate::metadata::{attr, cap, OpMetadata};

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
    pub(crate) fn append_wrapper(
        &mut self,
        root: NodeRef,
        platform: Option<pb::Platform>,
        custom_name: Option<&str>,
    ) -> Result<NodeRef, LlbError> {
        let wrapper_op = pb::Op {
            inputs: vec![pb::Input {
                digest: root.digest().as_str().to_string(),
                index: root.index().0 as i64,
            }],
            platform,
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
    use crate::platform::Platform;
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
        // One scratch + one mkdir (deduped) + merge + wrapper = 4 entries.
        assert_eq!(def.def.len(), 4);
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
        let s = scratch().unwrap();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        let wrapper_bytes = def.def.last().expect("definition is non-empty");
        let wrapper_op = pb::Op::decode(wrapper_bytes.as_slice()).unwrap();
        assert!(wrapper_op.op.is_none());
        assert_eq!(wrapper_op.inputs.len(), 1);
        assert_eq!(
            wrapper_op.platform,
            Some(pb::Platform {
                architecture: "amd64".to_string(),
                os: "linux".to_string(),
                variant: String::new(),
                os_version: String::new(),
                os_features: Vec::new(),
            })
        );
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

        let last_dgst = sha256(def.def.last().unwrap());
        assert_eq!(def.root.as_str(), last_dgst.as_str());
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
            .get(def.root.as_str())
            .expect("root has metadata");
        assert!(wrapper_md.caps.contains_key(cap::CAP_META_DESCRIPTION));
    }

    #[test]
    fn marshal_wrapper_platform_default_is_linux_amd64() {
        let s = scratch().unwrap();
        let def = s.marshal(MarshalOpts::default()).unwrap();
        let wrapper_op = pb::Op::decode(def.def.last().unwrap().as_slice()).unwrap();
        assert_eq!(
            wrapper_op.platform,
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
    fn marshal_wrapper_platform_none_when_opts_explicitly_none() {
        let s = scratch().unwrap();
        let def = s.marshal(MarshalOpts { platform: None }).unwrap();
        let wrapper_op = pb::Op::decode(def.def.last().unwrap().as_slice()).unwrap();
        assert_eq!(wrapper_op.platform, None);
    }

    #[test]
    fn marshal_wrapper_platform_linux_amd64() {
        let s = scratch().unwrap();
        let def = s.marshal(MarshalOpts::linux_amd64()).unwrap();
        let wrapper_op = pb::Op::decode(def.def.last().unwrap().as_slice()).unwrap();
        assert_eq!(
            wrapper_op.platform,
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
    fn marshal_wrapper_digest_differs_with_platform() {
        let s = scratch().unwrap();
        let def_default = s.marshal(MarshalOpts::default()).unwrap();
        let def_arm64 = s
            .marshal(MarshalOpts::default().with_platform(Platform::LINUX_ARM64.clone()))
            .unwrap();
        assert_ne!(def_default.root, def_arm64.root);
    }

    #[test]
    fn marshal_state_custom_name_in_wrapper_metadata() {
        let s = scratch().unwrap().with_custom_name("root wrapper name");
        let def = s.marshal(MarshalOpts::default()).unwrap();
        let wrapper_md = def
            .metadata
            .get(def.root.as_str())
            .expect("root has metadata");
        assert_eq!(
            wrapper_md.description.get(attr::DESCRIPTION_NAME),
            Some(&"root wrapper name".to_string())
        );
        assert!(wrapper_md.caps.contains_key(cap::CAP_META_DESCRIPTION));
    }

    #[test]
    fn marshal_constraints_platform_used_when_opts_platform_none() {
        let s = scratch()
            .unwrap()
            .with_platform(Platform::LINUX_ARM64.clone());
        let def = s.marshal(MarshalOpts { platform: None }).unwrap();
        let wrapper_op = pb::Op::decode(def.def.last().unwrap().as_slice()).unwrap();
        assert_eq!(
            wrapper_op.platform,
            Some(pb::Platform {
                architecture: "arm64".to_string(),
                os: "linux".to_string(),
                variant: String::new(),
                os_version: String::new(),
                os_features: Vec::new(),
            })
        );
    }

    #[test]
    fn marshal_opts_platform_overrides_constraints_platform() {
        let s = scratch()
            .unwrap()
            .with_platform(Platform::LINUX_ARM64.clone());
        let def = s.marshal(MarshalOpts::linux_amd64()).unwrap();
        let wrapper_op = pb::Op::decode(def.def.last().unwrap().as_slice()).unwrap();
        assert_eq!(
            wrapper_op.platform,
            Some(pb::Platform {
                architecture: "amd64".to_string(),
                os: "linux".to_string(),
                variant: String::new(),
                os_version: String::new(),
                os_features: Vec::new(),
            })
        );
    }
}
