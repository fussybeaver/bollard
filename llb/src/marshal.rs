//! Digest and deterministic-serialization helpers for LLB operations.
//!
//! **Note on determinism:** content-digest dedup assumes that two equivalent
//! operations serialize to identical bytes. `prost` encodes `HashMap` fields in
//! their native iteration order, which is not stable across processes. For ops
//! that carry maps (`SourceOp.attrs`, `OpMetadata.description`,
//! `OpMetadata.caps`) this must be addressed before Phase 2, typically by
//! generating those fields as `BTreeMap` in `bollard-buildkit-proto`.

use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

use bollard_buildkit_proto::pb;
use prost::Message;
use sha2::{Digest as Sha2Digest, Sha256};

use crate::error::LlbError;

/// A content digest of the form `sha256:<64-hex-chars>`.
///
/// Cheap to clone (`Arc`-backed) and used as the key for operation dedup.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Digest(Arc<str>);

impl Digest {
    /// Returns the raw `sha256:...` string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Digest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Digest {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Compute the SHA-256 digest of arbitrary bytes.
pub fn sha256(bytes: &[u8]) -> Digest {
    let hash = Sha256::digest(bytes);
    Digest(Arc::from(format!("sha256:{}", hex::encode(hash))))
}

/// Deterministically encode a protobuf `Op` into bytes.
///
/// The encoded form is used both for transport and for content-digest
/// computation.
pub fn encode_op(op: &pb::Op) -> Result<Vec<u8>, LlbError> {
    let mut buf = Vec::new();
    op.encode(&mut buf).map_err(|source| LlbError::Encode {
        op: "Op".to_string(),
        source,
    })?;
    Ok(buf)
}

/// Deterministically encode a protobuf `Op` and return both its SHA-256 digest
/// and the encoded bytes.
///
/// This encodes the op exactly once, hashes the resulting buffer, and then
/// returns the same buffer for storage. Callers must ensure the encoded form is
/// deterministic (see module note).
pub fn encode_and_hash(op: &pb::Op) -> Result<(Digest, Vec<u8>), LlbError> {
    let mut bytes = Vec::new();
    op.encode(&mut bytes).map_err(|source| LlbError::Encode {
        op: "Op".to_string(),
        source,
    })?;
    Ok((sha256(&bytes), bytes))
}
