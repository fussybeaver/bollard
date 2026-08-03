//! The marshalled LLB graph.
//!
//! A [`Definition`] is the end result of [`State::marshal`](crate::state::State::marshal).
//! It can be converted into a protobuf [`bollard_buildkit_proto::pb::Definition`]
//! and then encoded to bytes for a BuildKit `SolveRequest`.

use std::collections::BTreeMap;
use std::io::{self, Write};

use bollard_buildkit_proto::pb;
use prost::Message;

use crate::error::LlbError;
use crate::marshal::Digest;

/// A marshalled LLB graph.
///
/// The `def` field is a topologically sorted list of serialized [`pb::Op`]
/// vertices: children precede parents, and the final entry is a synthetic
/// wrapper vertex that points at the real head.
///
#[derive(Clone, Debug)]
pub struct Definition {
    /// Serialized [`pb::Op`] bytes in topological (post-order) order.
    pub def: Vec<Vec<u8>>,

    /// Per-vertex metadata keyed by op digest string.
    pub metadata: BTreeMap<String, pb::OpMetadata>,

    /// Source mapping information for real vertices. Entries without explicit
    /// user locations contain an empty [`pb::Locations`] value.
    pub source: Option<pb::Source>,

    /// Digest of the real graph head referenced by the final wrapper.
    ///
    /// This is `None` for an empty scratch definition. The wrapper digest is
    /// intentionally not exposed as the graph head.
    pub root: Option<Digest>,
}

impl Definition {
    /// Convert this definition into a protobuf [`pb::Definition`].
    pub fn to_pb(&self) -> pb::Definition {
        pb::Definition {
            def: self.def.clone(),
            metadata: self.metadata.clone(),
            source: self.source.clone(),
        }
    }

    /// Encode this definition as protobuf bytes.
    pub fn into_bytes(self) -> Result<Vec<u8>, LlbError> {
        let definition = pb::Definition {
            def: self.def,
            metadata: self.metadata,
            source: self.source,
        };
        let mut buf = Vec::with_capacity(definition.encoded_len());
        definition
            .encode(&mut buf)
            .map_err(|source| LlbError::Encode {
                op: "Definition".to_string(),
                source,
            })?;
        Ok(buf)
    }

    /// Write this definition as binary protobuf to a writer.
    ///
    /// Write fields incrementally without cloning the complete definition or
    /// buffering the complete encoded output.
    pub fn write_to<W: io::Write>(&self, w: &mut W) -> Result<(), LlbError> {
        for bytes in &self.def {
            write_bytes_field(w, 1, bytes)?;
        }

        for (key, metadata) in &self.metadata {
            let key_len = field_len(1, key.len());
            let value_len = field_len(2, metadata.encoded_len());
            write_key_and_length(w, 2, key_len + value_len)?;
            write_bytes_field(w, 1, key.as_bytes())?;
            write_message_field(w, 2, metadata)?;
        }

        if let Some(source) = &self.source {
            write_message_field(w, 3, source)?;
        }

        Ok(())
    }
}

fn write_message_field<W: Write, M: prost::Message>(
    w: &mut W,
    tag: u32,
    message: &M,
) -> Result<(), LlbError> {
    let encoded = message.encode_to_vec();
    write_length_delimited(w, tag, &encoded)
}

fn write_bytes_field<W: Write>(w: &mut W, tag: u32, bytes: &[u8]) -> Result<(), LlbError> {
    write_length_delimited(w, tag, bytes)
}

fn write_length_delimited<W: Write>(w: &mut W, tag: u32, bytes: &[u8]) -> Result<(), LlbError> {
    write_key_and_length(w, tag, bytes.len())?;
    w.write_all(bytes)?;
    Ok(())
}

fn write_key_and_length<W: Write>(w: &mut W, tag: u32, length: usize) -> Result<(), LlbError> {
    write_varint(w, u64::from((tag << 3) | 2))?;
    write_varint(w, length as u64)
}

fn write_varint<W: Write>(w: &mut W, mut value: u64) -> Result<(), LlbError> {
    let mut encoded = [0u8; 10];
    let mut length = 0;
    while value >= 0x80 {
        encoded[length] = (value as u8) | 0x80;
        length += 1;
        value >>= 7;
    }
    encoded[length] = value as u8;
    w.write_all(&encoded[..=length])?;
    Ok(())
}

fn field_len(tag: u32, payload_len: usize) -> usize {
    varint_len(u64::from((tag << 3) | 2)) + varint_len(payload_len as u64) + payload_len
}

fn varint_len(mut value: u64) -> usize {
    let mut length = 1;
    while value >= 0x80 {
        length += 1;
        value >>= 7;
    }
    length
}
