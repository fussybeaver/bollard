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
and replaces the checked-in resources only after preparation succeeds. The
complete provenance lock remains a separate step; the update command does not
write a partial lock.

`check` generates into a temporary directory and compares the result byte-for-byte
with the checked-in Rust output without modifying the working tree. The online
variant additionally resolves, fetches, transforms, and compares every immutable
resource with the checked-in resource tree. Lock-backed verification is deferred
until the complete provenance lock is available:

```bash
cargo xtask buildkit check --online
```

The online check reports source and transformed-output hashes but does not yet
verify them against a committed provenance lock. Set `GITHUB_TOKEN` when using
the resolver in environments subject to GitHub API rate limits.

For development-only Moby source investigations, a mutable Moby branch can be
selected explicitly:

```bash
cargo xtask buildkit update --allow-moby-branch
```

This mode is warned as mutable and must not be used to prepare release
provenance. The default update command remains tag-only.

The generated files remain checked in so Bollard consumers do not need a
`protoc` installation. A `protoc` compiler is required when running the update
command locally.

## Transitional legacy commands

The original feature-gated commands remain temporarily during the tooling
migration. They are not provenance-safe because they fetch branch-head sources,
must not be extended or used as the source of a release, and are scheduled for
removal once the lock-backed workflow is complete.

```
cargo run --bin fetch --features fetch
cargo run --bin gen --features build
```
