#![cfg(feature = "buildkit")]

/// Errors related to the Grpc functionality
#[derive(Debug, thiserror::Error)]
pub enum GrpcError {
    /// Generic error emitted by the bollard codebase.
    #[error(transparent)]
    DockerError {
        /// The original error of the bollard codebase.
        #[from]
        err: crate::errors::Error,
    },
    /// Error emitted when log output generates an I/O error.
    #[error(transparent)]
    StrParseError {
        /// The original error emitted.
        #[from]
        err: std::str::Utf8Error,
    },
    /// Error emitted when a GRPC network request or response fails with docker's buildkit client
    #[error(transparent)]
    TonicError {
        /// The tonic error emitted.
        #[from]
        err: tonic::transport::Error,
    },
    /// Error emitted when a GRPC network request emits a non-OK status code
    #[error(transparent)]
    TonicStatus {
        /// The tonic status emitted.
        #[from]
        err: tonic::Status,
    },
    /// Error emitted when a GRPC metadata value does not parse correctly
    #[error(transparent)]
    MetadataValue {
        /// The tonic metadata value.
        #[from]
        err: tonic::metadata::errors::InvalidMetadataValue,
    },
}
