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
