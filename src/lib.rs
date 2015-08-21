//! Docker
#![doc(html_root_url="https://ghmlee.github.io/rust-docker/doc")]

// import external libliries
extern crate openssl;
extern crate unix_socket;
extern crate rustc_serialize;

// declare modules
mod tcp;
mod unix;
mod http;
mod test;
mod docker;
pub mod container;
pub mod stats;
pub mod system;
pub mod image;
pub mod process;

// publicly re-export
pub use docker::Docker;
