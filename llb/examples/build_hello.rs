//! Basic "hello world" example demonstrating BuildKit LLB emission.
//!
//! This example creates an LLB definition that:
//! 1. Pulls an Alpine Linux image
//! 2. Runs `echo 'hello world'` inside the container
//! 3. Writes the binary protobuf definition to stdout
//!
//! Usage:
//!   cargo run --example build_hello --package bollard-llb | \
//!     buildctl build --progress plain --no-cache

use bollard_llb::{image, shlex};

fn main() {
    let def = image("alpine:latest")
        .run(shlex("echo 'hello world'"))
        .with_custom_name("echo hello world")
        .root()
        .marshal(Default::default())
        .unwrap();

    def.write_to(&mut std::io::stdout()).unwrap();
}
