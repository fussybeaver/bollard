//! Docker
#![doc(html_root_url = "https://ghmlee.github.io/rust-docker/doc")]
// Increase the compiler's recursion limit for the `error_chain` crate.
#![recursion_limit = "1024"]
//#![deny(missing_docs)]

// import external libraries
#[macro_use]
extern crate failure;
extern crate futures;
extern crate http;
extern crate hyper;
extern crate hyper_tls;
extern crate native_tls;
#[cfg(feature = "openssl")]
extern crate openssl;
#[macro_use]
extern crate serde_derive;
extern crate bytes;
extern crate hex;
#[cfg(feature = "openssl")]
extern crate hyper_openssl;
#[cfg(unix)]
extern crate hyperlocal;
extern crate mio;
#[cfg(windows)]
extern crate mio_named_pipes;
extern crate serde;
extern crate serde_json;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_reactor;
extern crate url;
#[cfg(windows)]
extern crate winapi;

// declare modules
pub mod container;
mod docker;
pub mod errors;
pub mod filesystem;
pub mod image;
pub mod named_pipe;
mod options;
pub mod process;
pub mod stats;
pub mod system;
mod test;
mod util;
pub mod version;

// publicly re-export
pub use docker::Docker;
pub use options::*;
