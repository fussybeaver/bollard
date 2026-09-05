//! Human-readable and JSON dumping of LLB [`Definition`]s.
//!
//! The binary encoding of a definition is provided by
//! [`Definition::write_to`](crate::definition::Definition::write_to). This
//! module contains the shared protobuf traversal used by the debugging dumps.

use bollard_buildkit_proto::pb;
use prost::Message;

use crate::definition::Definition;
use crate::error::LlbError;
use crate::marshal::sha256;

#[cfg(feature = "dump_json")]
mod json;
mod text;

#[cfg(feature = "dump_json")]
pub use json::dump_json;
pub use text::dump_text;

pub(crate) fn for_each_op<F>(
    def: &Definition,
    mut visit: F,
    op_name: &'static str,
) -> Result<(), LlbError>
where
    F: FnMut(&crate::marshal::Digest, &pb::Op, Option<&pb::OpMetadata>) -> Result<(), LlbError>,
{
    for bytes in &def.def {
        let op = pb::Op::decode(bytes.as_slice()).map_err(|source| LlbError::Decode {
            op: op_name.to_string(),
            source,
        })?;
        let digest = sha256(bytes);
        visit(&digest, &op, def.metadata.get(digest.as_str()))?;
    }
    Ok(())
}

pub(crate) fn mount_type_name(value: i32) -> &'static str {
    pb::MountType::try_from(value)
        .map(|t| t.as_str_name())
        .unwrap_or("UNKNOWN")
}

pub(crate) fn net_mode_name(value: i32) -> &'static str {
    pb::NetMode::try_from(value)
        .map(|t| t.as_str_name())
        .unwrap_or("UNKNOWN")
}

pub(crate) fn security_mode_name(value: i32) -> &'static str {
    pb::SecurityMode::try_from(value)
        .map(|t| t.as_str_name())
        .unwrap_or("UNKNOWN")
}
