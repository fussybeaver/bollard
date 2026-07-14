//! `bollard-llb`: construct BuildKit Low-Level Builder (LLB) graphs.
//!
//! This crate provides a synchronous, dependency-light DSL for emitting
//! [`bollard_buildkit_proto::pb::Definition`] values. It is intended to be
//! usable on its own for tools that only need to *produce* LLB (e.g. to feed
//! `buildctl build`), and is re-exported by `bollard` as `bollard::llb`.
//!
//! The crate is intentionally async-free: it does not depend on `tokio`,
//! `tonic`, or any TLS stack.

#![deny(
    missing_debug_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces
)]
#![warn(missing_docs, rust_2018_idioms)]
#![allow(clippy::upper_case_acronyms, dead_code)]

/// Error types produced when marshalling LLB graphs.
pub mod error;
/// Digest and serialization helpers.
pub(crate) mod marshal;
/// Per-vertex metadata and well-known constants.
pub(crate) mod metadata;
/// Core operation graph types.
pub(crate) mod ops;

pub use error::LlbError;
pub use marshal::Digest;
