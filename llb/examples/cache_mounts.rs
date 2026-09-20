//! Cache mount example demonstrating all three cache sharing modes.
//!
//! This example creates an LLB definition with three exec steps, each using a
//! different cache mount sharing mode:
//! 1. `Shared` - cache is shared between concurrent builds
//! 2. `Private` - cache is private to this build
//! 3. `Locked` - exclusive access, only one build at a time
//!
//! Usage:
//!   cargo run --example cache_mounts --package bollard-llb | \
//!     buildctl build --progress plain --no-cache

use bollard_llb::{image, shlex, CacheSharingMode};

fn main() {
    let st = image("alpine:latest")
        .unwrap()
        .run(shlex("apk add --no-cache curl git jq").unwrap())
        .with_custom_name("install packages with cached apk")
        .add_mount_cache("/var/cache/apk", "apk-cache", CacheSharingMode::Shared)
        .root()
        .unwrap();

    let st = st
        .run(shlex(
            r#"sh -c "echo 'build artifact' > /tmp/build-cache/result.txt && cat /tmp/build-cache/result.txt""#,
        ).unwrap())
        .with_custom_name("build step with private cache")
        .add_mount_cache(
            "/tmp/build-cache",
            "build-cache-v1",
            CacheSharingMode::Private,
        )
        .root()
        .unwrap();

    let st = st
        .run(shlex(
            r#"sh -c "echo 'writing to locked cache' > /tmp/locked/state.txt && cat /tmp/locked/state.txt""#,
        ).unwrap())
        .with_custom_name("finalize with locked cache")
        .add_mount_cache("/tmp/locked", "locked-state", CacheSharingMode::Locked)
        .root()
        .unwrap();

    let def = st.marshal(Default::default()).unwrap();
    def.write_to(&mut std::io::stdout()).unwrap();
}
