//! Error types for `bollard-llb`.

use thiserror::Error;

/// Errors that can occur while constructing or marshalling an LLB graph.
#[derive(Debug, Error)]
pub enum LlbError {
    /// Failed to encode an operation into its protobuf representation.
    #[error("failed to encode protobuf for {op}: {source}")]
    Encode {
        /// Human-readable operation name used in diagnostics.
        op: String,
        /// The underlying prost encode error.
        #[source]
        source: prost::EncodeError,
    },

    /// Failed to decode a protobuf message.
    #[error("failed to decode protobuf for {op}: {source}")]
    Decode {
        /// Human-readable operation name used in diagnostics.
        op: String,
        /// The underlying prost decode error.
        #[source]
        source: prost::DecodeError,
    },

    /// An operation referenced an input that was not registered.
    #[error("missing input for operation")]
    MissingInput,

    /// An image or other source reference could not be parsed.
    #[error("invalid image reference {reference:?}")]
    InvalidReference {
        /// The reference that failed validation.
        reference: String,
    },

    /// A shell command could not be split into arguments.
    #[error("invalid shell command at byte {position}: {kind}")]
    InvalidShell {
        /// Byte offset of the unmatched quote or trailing escape.
        position: usize,
        /// Description of the unterminated shell construct.
        kind: &'static str,
    },

    /// An exec operation is missing its command arguments.
    #[error("invalid exec operation: {reason}")]
    InvalidExec {
        /// Description of the invalid exec operation.
        reason: &'static str,
    },

    /// An I/O error occurred while writing the LLB dump.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A feature or operation has not been implemented yet.
    #[error("not yet implemented: {0}")]
    Unimplemented(&'static str),

    /// Failed to serialize a value to an intermediate representation.
    #[error("serialization error: {0}")]
    Serialization(String),
}
