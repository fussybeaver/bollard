# Buildkit rust proto

This repository contains both the protobuf descriptors and generated output.

The generated files are part of the project to maintain consistency across
generated assets in the Bollard project, and to avoid a build dependency on the
external `protoc` binary.

## BuildKit tooling

Run the preferred generation and verification commands from the repository root:

```bash
cargo xtask buildkit update
cargo xtask buildkit check
```

`update` first resolves the Moby release selected by `codegen/swagger/pom.xml`
to an immutable Moby commit, reads its direct BuildKit requirement from the
immutable `go.mod`, fetches the dependency-classified protobuf sources, applies
named transformations with exact match counts, and stages source and output
hashes. It regenerates the checked-in bindings from the transformed resources
and commits the checked-in resources, generated bindings, and provenance lock as
one rollback-protected update after preparation succeeds. A failed commit
restores the previous outputs.

`check` generates into a temporary directory and compares the result byte-for-byte
with the checked-in Rust output without modifying the working tree, and enforces
the committed provenance lock and resource hashes. The online variant additionally
resolves, fetches, transforms, and verifies every immutable source and its source
hash against the lock:

```bash
cargo xtask buildkit check --online
```

Set `GITHUB_TOKEN` when using the resolver in environments subject to GitHub API
rate limits.

The generated files remain checked in so Bollard consumers do not need a
generator or `protoc` installation. The unpublished xtask uses the pinned
vendored `protoc`; its generator dependencies and expected versions are
maintained in [`xtask/Cargo.toml`](xtask/Cargo.toml) and
[`xtask/src/provenance.rs`](xtask/src/provenance.rs). It records the generation
contract in [`provenance.lock.toml`](provenance.lock.toml). Do not set `PROTOC`
or `PROTOC_INCLUDE` while running xtask.

### Upgrading the generator toolchain

The generator dependencies are pinned exactly so the checked-in bindings can be
reproduced and their provenance verified. These pins can be upgraded, but the
dependency versions, provenance expectations, and generated outputs must be
updated together.

Dependabot groups `protoc-bin-vendored`, `tonic-prost-build`, and `prost-build`
updates. Its PRs update the Cargo manifest and lockfile, but do not update the
provenance expectations or regenerate outputs. A dependency-only update will
therefore fail `cargo xtask buildkit check` until the coordinated upgrade is
complete.

To upgrade the toolchain:

1. Update the relevant exact dependency pins in `xtask/Cargo.toml` and the Cargo
   lockfile.
2. Update the corresponding version constants in `xtask/src/provenance.rs`.
   When upgrading `protoc-bin-vendored`, also update `PROTOC_VERSION` to the
   actual compiler version bundled by that release; the crate and compiler
   version numbers are different.
3. With `PROTOC` and `PROTOC_INCLUDE` unset, regenerate the outputs:

   ```bash
   cargo xtask buildkit update
   ```

4. Review the generated bindings and `provenance.lock.toml`. A generator-only
   upgrade should preserve the Moby/BuildKit baseline and protobuf source hashes.
   Investigate any unexpected changes before proceeding.
5. Run the generation and test gates:

   ```bash
   cargo xtask buildkit check
   cargo test -p xtask
   cargo test -p bollard-buildkit-proto --all-features
   cargo test -p bollard-buildkit-proto --no-default-features
   cargo test -p bollard-llb --all-features
   ```

Commit the coordinated changes together and require the normal CI checks to pass
before merging. Do not remove the exact-version or provenance checks to make a
dependency-only update pass.

## Features

The `grpc` feature is enabled by default and exposes the generated gRPC clients
and servers. Disable default features when only protobuf messages and
provenance are needed; this avoids resolving the gRPC transport dependency
stack.
