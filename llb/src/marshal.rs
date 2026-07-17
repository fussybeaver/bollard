//! Digest and deterministic-serialization helpers for LLB operations.
//!
//! **Note on determinism:** content-digest dedup assumes that two equivalent
//! operations serialize to identical bytes. Map fields are generated as
//! `BTreeMap` in `bollard-buildkit-proto`; the top-level `pb::Op` also uses a
//! custom encoder because BuildKit's Go encoder writes its fields in a
//! different order from Prost's tag-sorted derive implementation.

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

    /// Sentinel digest used for empty scratch inputs.
    pub(crate) fn empty() -> Self {
        Digest(Arc::from(""))
    }

    /// Returns `true` for the empty scratch sentinel digest.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
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

/// Encode a protobuf `Op` using BuildKit's Go/vtprotobuf field order.
///
/// BuildKit writes repeated inputs first, followed by platform and worker
/// constraints, then the operation oneof. Prost sorts fields by tag and would
/// otherwise write the oneof before fields 10 and 11.
pub fn encode_op(op: &pb::Op) -> Result<Vec<u8>, LlbError> {
    let mut buf = Vec::new();

    for input in &op.inputs {
        encode_message_field(&mut buf, 1, input)?;
    }
    if let Some(platform) = &op.platform {
        encode_message_field(&mut buf, 10, platform)?;
    }
    if let Some(constraints) = &op.constraints {
        encode_message_field(&mut buf, 11, constraints)?;
    }
    if let Some(operation) = &op.op {
        match operation {
            pb::op::Op::Exec(exec) => encode_message_field(&mut buf, 2, exec)?,
            pb::op::Op::Source(source) => encode_message_field(&mut buf, 3, source)?,
            pb::op::Op::File(file) => encode_message_field(&mut buf, 4, file)?,
            pb::op::Op::Build(build) => encode_message_field(&mut buf, 5, build)?,
            pb::op::Op::Merge(merge) => encode_message_field(&mut buf, 6, merge)?,
            pb::op::Op::Diff(diff) => encode_message_field(&mut buf, 7, diff)?,
        }
    }
    Ok(buf)
}

fn encode_message_field<M: Message>(
    buf: &mut Vec<u8>,
    tag: u8,
    message: &M,
) -> Result<(), LlbError> {
    let mut encoded = Vec::new();
    message
        .encode(&mut encoded)
        .map_err(|source| LlbError::Encode {
            op: "Op".to_string(),
            source,
        })?;
    buf.push((tag << 3) | 2);
    encode_varint(buf, encoded.len() as u64);
    buf.extend_from_slice(&encoded);
    Ok(())
}

fn encode_varint(buf: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

/// Deterministically encode a protobuf `Op` and return both its SHA-256 digest
/// and the encoded bytes.
///
/// This encodes the op exactly once, hashes the resulting buffer, and then
/// returns the same buffer for storage. Callers must ensure the encoded form is
/// deterministic (see module note).
pub fn encode_and_hash(op: &pb::Op) -> Result<(Digest, Vec<u8>), LlbError> {
    let bytes = encode_op(op)?;
    Ok((sha256(&bytes), bytes))
}

#[cfg(test)]
mod tests {
    use bollard_buildkit_proto::pb;
    use prost::Message;

    use super::*;

    #[test]
    fn op_fields_use_buildkit_order() {
        let op = pb::Op {
            inputs: vec![pb::Input {
                digest: "sha256:input".to_string(),
                index: 0,
            }],
            platform: Some(pb::Platform {
                architecture: "amd64".to_string(),
                os: "linux".to_string(),
                ..Default::default()
            }),
            constraints: Some(pb::WorkerConstraints { filter: vec![] }),
            op: Some(pb::op::Op::Source(pb::SourceOp {
                identifier: "docker-image://alpine:latest".to_string(),
                attrs: Default::default(),
            })),
        };

        let encoded = encode_op(&op).unwrap();
        let platform_tag = encoded
            .iter()
            .position(|byte| *byte == 0x52)
            .expect("platform field tag");
        let constraints_tag = encoded
            .iter()
            .position(|byte| *byte == 0x5a)
            .expect("constraints field tag");
        let source_tag = encoded
            .iter()
            .position(|byte| *byte == 0x1a)
            .expect("source field tag");

        assert!(platform_tag < constraints_tag);
        assert!(constraints_tag < source_tag);
        assert_eq!(pb::Op::decode(encoded.as_slice()).unwrap(), op);
    }

    #[test]
    fn wrapper_encoding_matches_prost_without_reordered_fields() {
        let op = pb::Op {
            inputs: vec![pb::Input {
                digest: "sha256:input".to_string(),
                index: 0,
            }],
            platform: None,
            constraints: None,
            op: None,
        };
        let encoded = encode_op(&op).unwrap();
        let mut prost_encoded = Vec::new();
        op.encode(&mut prost_encoded).unwrap();
        assert_eq!(encoded, prost_encoded);
    }

    #[test]
    fn encode_and_hash_uses_stored_bytes() {
        let op = pb::Op {
            inputs: Vec::new(),
            platform: None,
            constraints: None,
            op: Some(pb::op::Op::Source(pb::SourceOp {
                identifier: "local://context".to_string(),
                attrs: Default::default(),
            })),
        };
        let (digest, bytes) = encode_and_hash(&op).unwrap();
        assert_eq!(digest, sha256(&bytes));
    }
}
