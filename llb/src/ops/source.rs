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
        let reference = normalize_image_reference(&reference.into())?;
        let attrs = BTreeMap::new();
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
        if mode == ResolveMode::Default {
            self.metadata
                .caps
                .remove(cap::CAP_SOURCE_IMAGE_RESOLVE_MODE);
            self.attrs.remove(attr::IMAGE_RESOLVE_MODE);
        } else {
            self.attrs.insert(
                attr::IMAGE_RESOLVE_MODE.to_string(),
                mode.as_str().to_string(),
            );
            if mode == ResolveMode::ForcePull {
                self.metadata
                    .caps
                    .insert(cap::CAP_SOURCE_IMAGE_RESOLVE_MODE.to_string());
            } else {
                self.metadata
                    .caps
                    .remove(cap::CAP_SOURCE_IMAGE_RESOLVE_MODE);
            }
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

    /// Set the platform for the image source and the state derived from it.
    pub fn with_platform(mut self, platform: Platform) -> Result<Self, LlbError> {
        self.platform = Some(platform);
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

const DEFAULT_IMAGE_DOMAIN: &str = "docker.io";
const DEFAULT_IMAGE_NAMESPACE: &str = "library";

fn normalize_image_reference(reference: &str) -> Result<String, LlbError> {
    if reference.is_empty()
        || reference
            .chars()
            .any(|c| c.is_ascii_whitespace() || c.is_control())
        || reference.split('@').count() > 2
    {
        return Err(LlbError::InvalidReference {
            reference: reference.to_string(),
        });
    }

    let (name_and_tag, digest) = match reference.split_once('@') {
        Some((name, digest)) if !name.is_empty() && !digest.is_empty() => {
            validate_digest(digest, reference)?;
            (name, Some(digest))
        }
        Some(_) => {
            return Err(LlbError::InvalidReference {
                reference: reference.to_string(),
            });
        }
        None => (reference, None),
    };

    let (name, tag) = split_image_tag(name_and_tag, reference)?;
    let (domain, remote) = split_image_domain(name);
    validate_image_domain(domain, reference)?;
    let remote = if domain == DEFAULT_IMAGE_DOMAIN && !remote.contains('/') {
        format!("{DEFAULT_IMAGE_NAMESPACE}/{remote}")
    } else {
        remote.to_string()
    };

    validate_image_name(&remote, reference)?;
    let mut normalized = format!("{domain}/{remote}");
    if let Some(tag) = tag {
        normalized.push(':');
        normalized.push_str(tag);
    } else if digest.is_none() {
        normalized.push_str(":latest");
    }
    if let Some(digest) = digest {
        normalized.push('@');
        normalized.push_str(digest);
    }
    Ok(normalized)
}

fn split_image_tag<'a>(
    reference: &'a str,
    original: &str,
) -> Result<(&'a str, Option<&'a str>), LlbError> {
    let slash = reference.rfind('/').unwrap_or(0);
    let tag_start = reference[slash..].find(':').map(|index| slash + index);
    let Some(tag_start) = tag_start else {
        return Ok((reference, None));
    };
    let (name, tag) = reference.split_at(tag_start);
    let tag = &tag[1..];
    if tag.is_empty()
        || tag.len() > 128
        || !tag.as_bytes()[0].is_ascii_alphanumeric()
        || !tag
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == b'-')
    {
        return Err(LlbError::InvalidReference {
            reference: original.to_string(),
        });
    }
    Ok((name, Some(tag)))
}

fn split_image_domain(reference: &str) -> (&str, &str) {
    let Some((first, remainder)) = reference.split_once('/') else {
        return (DEFAULT_IMAGE_DOMAIN, reference);
    };
    if first == "localhost"
        || first.contains('.')
        || first.contains(':')
        || first.chars().any(|c| c.is_ascii_uppercase())
    {
        let domain = if first == "index.docker.io" {
            DEFAULT_IMAGE_DOMAIN
        } else {
            first
        };
        (domain, remainder)
    } else {
        (DEFAULT_IMAGE_DOMAIN, reference)
    }
}

fn validate_image_name(name: &str, original: &str) -> Result<(), LlbError> {
    if name.is_empty()
        || name.len() > 255
        || (name.len() == 64 && name.bytes().all(|c| c.is_ascii_hexdigit()))
        || name
            .split('/')
            .any(|component| !valid_image_component(component))
    {
        return Err(LlbError::InvalidReference {
            reference: original.to_string(),
        });
    }
    Ok(())
}

fn validate_image_domain(domain: &str, original: &str) -> Result<(), LlbError> {
    if domain == "localhost" {
        return Ok(());
    }

    if domain.starts_with('[') {
        let Some(end) = domain.find(']') else {
            return Err(LlbError::InvalidReference {
                reference: original.to_string(),
            });
        };
        let address = &domain[1..end];
        let port = &domain[end + 1..];
        if address.is_empty()
            || !address.bytes().all(|c| c.is_ascii_hexdigit() || c == b':')
            || (!port.is_empty()
                && (!port.starts_with(':') || !port[1..].bytes().all(|c| c.is_ascii_digit())))
        {
            return Err(LlbError::InvalidReference {
                reference: original.to_string(),
            });
        }
        return Ok(());
    }

    let (host, port) = match domain.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (domain, None),
    };
    let invalid_port =
        port.is_some_and(|port| port.is_empty() || !port.bytes().all(|c| c.is_ascii_digit()));
    if host.is_empty()
        || invalid_port
        || host
            .split('.')
            .any(|component| !valid_domain_component(component))
    {
        return Err(LlbError::InvalidReference {
            reference: original.to_string(),
        });
    }
    Ok(())
}

fn valid_domain_component(component: &str) -> bool {
    let bytes = component.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'-')
}

fn valid_image_component(component: &str) -> bool {
    let bytes = component.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_lowercase() || bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        match bytes[index] {
            b'.' => {
                index += 1;
                if index == bytes.len()
                    || !bytes[index].is_ascii_lowercase() && !bytes[index].is_ascii_digit()
                {
                    return false;
                }
            }
            b'_' => {
                index += 1;
                if index < bytes.len() && bytes[index] == b'_' {
                    index += 1;
                }
                if index == bytes.len()
                    || !bytes[index].is_ascii_lowercase() && !bytes[index].is_ascii_digit()
                {
                    return false;
                }
            }
            b'-' => {
                while index < bytes.len() && bytes[index] == b'-' {
                    index += 1;
                }
                if index == bytes.len()
                    || !bytes[index].is_ascii_lowercase() && !bytes[index].is_ascii_digit()
                {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

fn validate_digest(digest: &str, original: &str) -> Result<(), LlbError> {
    let Some((algorithm, encoded)) = digest.split_once(':') else {
        return Err(LlbError::InvalidReference {
            reference: original.to_string(),
        });
    };
    if algorithm.is_empty()
        || !algorithm.as_bytes()[0].is_ascii_alphabetic()
        || !algorithm
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'+' | b'.'))
        || encoded.len() < 32
        || !encoded.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(LlbError::InvalidReference {
            reference: original.to_string(),
        });
    }
    Ok(())
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
        let mut attrs = self.attrs.clone();
        let mut metadata = self.metadata.clone();
        // BuildKit uses LocalUniqueID only as a fallback when no session ID
        // is supplied. A session ID therefore suppresses the unique-id attr.
        if attrs.contains_key(attr::LOCAL_SESSION_ID) {
            attrs.remove(attr::LOCAL_UNIQUE_ID);
            metadata.caps.remove(cap::CAP_SOURCE_LOCAL_UNIQUE);
        }
        let pb_op = pb::Op {
            inputs: Vec::new(),
            platform: None,
            constraints: Some(pb::WorkerConstraints {
                filter: ctx.worker_filters().to_vec(),
            }),
            op: Some(pb::op::Op::Source(pb::SourceOp {
                identifier: self.identifier.clone(),
                attrs,
            })),
        };
        let (digest, bytes) = encode_and_hash(&pb_op)?;
        Ok(ctx.insert_node(Node {
            bytes,
            digest,
            metadata,
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
        let platform = image.platform.clone();
        State::with_constraints(
            OperationOutput::Owned(Arc::new(image)),
            platform.map_or_else(Default::default, |platform| {
                crate::state::Constraints::default().with_platform(platform)
            }),
        )
    }
}

impl From<Local> for State {
    fn from(local: Local) -> Self {
        State::new(OperationOutput::Owned(Arc::new(local)))
    }
}
