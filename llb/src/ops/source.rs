//! Source-operation builders: [`Image`], [`Local`], and [`Scratch`].

use std::collections::BTreeMap;
use std::sync::Arc;

use bollard_buildkit_proto::pb;

use crate::error::LlbError;
use crate::marshal::encode_and_hash;
use crate::metadata::{attr, cap, OpMetadata};
use crate::ops::{Context, Node, NodeRef, Operation, OperationOutput};
use crate::platform::Platform;
use crate::State;

/// A Docker image source.
#[derive(Clone, Debug)]
pub struct Image {
    reference: String,
    attrs: BTreeMap<String, String>,
    platform: Option<Platform>,
    metadata: OpMetadata,
}

impl Image {
    /// Create a new image source.
    pub fn new<S: Into<String>>(reference: S) -> Result<Self, LlbError> {
        let reference = reference.into();
        let mut attrs = BTreeMap::new();
        attrs.insert(
            attr::IMAGE_RESOLVE_MODE.to_string(),
            attr::IMAGE_RESOLVE_MODE_DEFAULT.to_string(),
        );
        let mut metadata = OpMetadata::default();
        metadata.caps.insert(cap::CAP_SOURCE_IMAGE.to_string());
        Ok(Self {
            reference,
            attrs,
            platform: None,
            metadata,
        })
    }

    /// Set the resolve mode for the image.
    pub fn with_resolve_mode(mut self, mode: ResolveMode) -> Result<Self, LlbError> {
        let value = mode.as_str();
        self.attrs
            .insert(attr::IMAGE_RESOLVE_MODE.to_string(), value.to_string());
        if mode == ResolveMode::Default {
            self.metadata
                .caps
                .remove(cap::CAP_SOURCE_IMAGE_RESOLVE_MODE);
        } else {
            self.metadata
                .caps
                .insert(cap::CAP_SOURCE_IMAGE_RESOLVE_MODE.to_string());
        }
        Ok(self)
    }

    /// Set the layer limit for the image.
    pub fn with_layer_limit(mut self, limit: u32) -> Result<Self, LlbError> {
        self.attrs
            .insert(attr::IMAGE_LAYER_LIMIT.to_string(), limit.to_string());
        self.metadata
            .caps
            .insert(cap::CAP_SOURCE_IMAGE_LAYER_LIMIT.to_string());
        Ok(self)
    }

    /// Set a checksum for the image.
    pub fn with_checksum<S: Into<String>>(mut self, checksum: S) -> Result<Self, LlbError> {
        let checksum = checksum.into();
        self.attrs
            .insert(attr::IMAGE_CHECKSUM.to_string(), checksum);
        self.metadata
            .caps
            .insert(cap::CAP_SOURCE_IMAGE_CHECKSUM.to_string());
        Ok(self)
    }

    /// Set the platform for the image source.
    pub fn with_platform(mut self, platform: Platform) -> Result<Self, LlbError> {
        self.platform = Some(platform);
        self.metadata.caps.insert(cap::CAP_PLATFORM.to_string());
        Ok(self)
    }

    /// Set a custom name for this image source.
    pub fn with_custom_name<S: Into<String>>(mut self, name: S) -> Result<Self, LlbError> {
        self.metadata
            .description
            .insert(attr::DESCRIPTION_NAME.to_string(), name.into());
        Ok(self)
    }
}

impl Operation for Image {
    fn serialize(&self, ctx: &mut Context) -> Result<NodeRef, LlbError> {
        let platform = ctx.combined_platform(self.platform.clone()).map(Into::into);
        let pb_op = pb::Op {
            inputs: Vec::new(),
            platform,
            constraints: Some(pb::WorkerConstraints {
                filter: ctx.worker_filters().to_vec(),
            }),
            op: Some(pb::op::Op::Source(pb::SourceOp {
                identifier: format!("docker-image://{}", self.reference),
                attrs: self.attrs.clone(),
            })),
        };
        let (digest, bytes) = encode_and_hash(&pb_op)?;
        Ok(ctx.insert_node(Node {
            bytes,
            digest,
            metadata: self.metadata.clone(),
        }))
    }
}

/// Image resolve modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolveMode {
    /// Default resolver behavior.
    Default,
    /// Always pull the image.
    ForcePull,
    /// Prefer a local image if available.
    PreferLocal,
}

impl ResolveMode {
    fn as_str(&self) -> &'static str {
        match self {
            ResolveMode::Default => attr::IMAGE_RESOLVE_MODE_DEFAULT,
            ResolveMode::ForcePull => attr::IMAGE_RESOLVE_MODE_FORCE_PULL,
            ResolveMode::PreferLocal => attr::IMAGE_RESOLVE_MODE_PREFER_LOCAL,
        }
    }
}

/// A local build-context source.
#[derive(Clone, Debug)]
pub struct Local {
    name: String,
    identifier: String,
    attrs: BTreeMap<String, String>,
    metadata: OpMetadata,
}

impl Local {
    /// Create a new local source.
    pub fn new<S: Into<String>>(name: S) -> Result<Self, LlbError> {
        let name = name.into();
        let identifier = format!("local://{name}");
        let mut metadata = OpMetadata::default();
        metadata.caps.insert(cap::CAP_SOURCE_LOCAL.to_string());
        Ok(Self {
            name,
            identifier,
            attrs: BTreeMap::new(),
            metadata,
        })
    }

    /// Set follow-paths for the local source.
    ///
    /// Encoded as a JSON array to match Go's `llb.FollowPaths`.
    pub fn with_follow_paths<I, S>(mut self, paths: I) -> Result<Self, LlbError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let paths: Vec<String> = paths.into_iter().map(|p| p.into()).collect();
        if !paths.is_empty() {
            let value = serde_json::to_string(&paths)
                .map_err(|e| LlbError::Serialization(e.to_string()))?;
            self.attrs
                .insert(attr::LOCAL_FOLLOW_PATHS.to_string(), value);
            self.metadata
                .caps
                .insert(cap::CAP_SOURCE_LOCAL_FOLLOW_PATHS.to_string());
        }
        Ok(self)
    }

    /// Set the session ID for the local source.
    pub fn with_session_id<S: Into<String>>(mut self, id: S) -> Result<Self, LlbError> {
        self.attrs
            .insert(attr::LOCAL_SESSION_ID.to_string(), id.into());
        self.metadata
            .caps
            .insert(cap::CAP_SOURCE_LOCAL_SESSION_ID.to_string());
        Ok(self)
    }

    /// Set the shared-key hint for the local source.
    pub fn with_shared_key_hint<S: Into<String>>(mut self, hint: S) -> Result<Self, LlbError> {
        self.attrs
            .insert(attr::LOCAL_SHARED_KEY_HINT.to_string(), hint.into());
        self.metadata
            .caps
            .insert(cap::CAP_SOURCE_LOCAL_SHARED_KEY_HINT.to_string());
        Ok(self)
    }

    /// Set the unique ID for the local source.
    pub fn with_unique_id<S: Into<String>>(mut self, id: S) -> Result<Self, LlbError> {
        self.attrs
            .insert(attr::LOCAL_UNIQUE_ID.to_string(), id.into());
        self.metadata
            .caps
            .insert(cap::CAP_SOURCE_LOCAL_UNIQUE.to_string());
        Ok(self)
    }

    /// Set include patterns for the local source.
    ///
    /// Encoded as a JSON array to match Go's `llb.IncludePatterns`.
    pub fn with_include_patterns<I, S>(mut self, patterns: I) -> Result<Self, LlbError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let patterns: Vec<String> = patterns.into_iter().map(|p| p.into()).collect();
        if !patterns.is_empty() {
            let value = serde_json::to_string(&patterns)
                .map_err(|e| LlbError::Serialization(e.to_string()))?;
            self.attrs
                .insert(attr::LOCAL_INCLUDE_PATTERNS.to_string(), value);
            self.metadata
                .caps
                .insert(cap::CAP_SOURCE_LOCAL_INCLUDE_PATTERNS.to_string());
        }
        Ok(self)
    }

    /// Set exclude patterns for the local source.
    ///
    /// Encoded as a JSON array to match Go's `llb.ExcludePatterns`.
    pub fn with_exclude_patterns<I, S>(mut self, patterns: I) -> Result<Self, LlbError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let patterns: Vec<String> = patterns.into_iter().map(|p| p.into()).collect();
        if !patterns.is_empty() {
            let value = serde_json::to_string(&patterns)
                .map_err(|e| LlbError::Serialization(e.to_string()))?;
            self.attrs
                .insert(attr::LOCAL_EXCLUDE_PATTERNS.to_string(), value);
            self.metadata
                .caps
                .insert(cap::CAP_SOURCE_LOCAL_EXCLUDE_PATTERNS.to_string());
        }
        Ok(self)
    }

    /// Set a custom name for this local source.
    pub fn with_custom_name<S: Into<String>>(mut self, name: S) -> Result<Self, LlbError> {
        self.metadata
            .description
            .insert(attr::DESCRIPTION_NAME.to_string(), name.into());
        Ok(self)
    }
}

impl Operation for Local {
    fn serialize(&self, ctx: &mut Context) -> Result<NodeRef, LlbError> {
        let pb_op = pb::Op {
            inputs: Vec::new(),
            platform: None,
            constraints: Some(pb::WorkerConstraints {
                filter: ctx.worker_filters().to_vec(),
            }),
            op: Some(pb::op::Op::Source(pb::SourceOp {
                identifier: self.identifier.clone(),
                attrs: self.attrs.clone(),
            })),
        };
        let (digest, bytes) = encode_and_hash(&pb_op)?;
        Ok(ctx.insert_node(Node {
            bytes,
            digest,
            metadata: self.metadata.clone(),
        }))
    }
}

/// Create a state backed by a Docker image.
pub fn image<S: Into<String>>(reference: S) -> Result<State, LlbError> {
    Ok(State::new(OperationOutput::Owned(Arc::new(Image::new(
        reference,
    )?))))
}

/// Create a state backed by a local build context.
pub fn local<S: Into<String>>(name: S) -> Result<State, LlbError> {
    Ok(State::new(OperationOutput::Owned(Arc::new(Local::new(
        name,
    )?))))
}

/// Create an empty scratch state.
pub fn scratch() -> Result<State, LlbError> {
    Ok(State::new(OperationOutput::Empty))
}

impl From<Image> for State {
    fn from(image: Image) -> Self {
        State::new(OperationOutput::Owned(Arc::new(image)))
    }
}

impl From<Local> for State {
    fn from(local: Local) -> Self {
        State::new(OperationOutput::Owned(Arc::new(local)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MarshalOpts;

    fn image_digest(image: Image) -> String {
        State::from(image)
            .marshal(MarshalOpts::default())
            .unwrap()
            .root
            .to_string()
    }

    #[test]
    fn image_digest_stable() {
        let a = image_digest(Image::new("alpine:latest").unwrap());
        let b = image_digest(Image::new("alpine:latest").unwrap());
        assert_eq!(a, b);
    }

    #[test]
    fn image_digest_differs_by_resolve_mode() {
        let a = image_digest(Image::new("alpine:latest").unwrap());
        let b = image_digest(
            Image::new("alpine:latest")
                .unwrap()
                .with_resolve_mode(ResolveMode::ForcePull)
                .unwrap(),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn local_follow_paths_encoded() {
        let local = Local::new("context")
            .unwrap()
            .with_follow_paths(["src", "Cargo.toml"])
            .unwrap();
        let expected = r#"["src","Cargo.toml"]"#;
        assert_eq!(
            local.attrs.get(attr::LOCAL_FOLLOW_PATHS),
            Some(&expected.to_string())
        );
        assert!(local
            .metadata
            .caps
            .contains(cap::CAP_SOURCE_LOCAL_FOLLOW_PATHS));
    }

    #[test]
    fn scratch_is_empty_output() {
        let a = scratch().unwrap();
        let b = scratch().unwrap();
        assert!(a.output().is_empty());
        assert!(b.output().is_empty());
    }
}
