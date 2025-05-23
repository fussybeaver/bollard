#![allow(
    missing_docs,
    trivial_casts,
    unused_variables,
    unused_mut,
    unused_imports,
    unused_extern_crates,
    non_camel_case_types
)]

use std::io::Error;

#[allow(unused_imports)]
use std::collections::HashMap;

pub const BASE_PATH: &str = "/v1.48";

pub mod query_parameters;
pub mod models;

#[cfg(feature = "buildkit")]
pub use bollard_buildkit_proto::moby;
