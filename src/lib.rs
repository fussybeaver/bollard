//! Docker
#![doc(html_root_url="https://ghmlee.github.io/rust-docker/doc")]

// import external libraries
extern crate hyper;
extern crate openssl;
extern crate unix_socket;
extern crate rustc_serialize;

// declare modules
mod test;
mod util;
mod unix;
mod docker;
pub mod container;
pub mod stats;
pub mod system;
pub mod image;
pub mod process;
pub mod filesystem;
pub mod version;

// publicly re-export
pub use docker::Docker;
