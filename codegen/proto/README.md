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
immutable `go.mod`, reports the derived BuildKit baseline, and stages and hashes
the dependency-classified protobuf sources. It then regenerates the checked-in
bindings from the committed resources. Source transformations, checked-in
resource replacement, and the complete provenance lock remain separate steps;
the update command deliberately does not write a partial lock.

`check` generates into a temporary directory and compares the result byte-for-byte
with the checked-in Rust output without modifying the working tree. The
provenance-aware online check remains disabled until the complete provenance lock
is available:

```bash
cargo xtask buildkit check --online
```

The xtask currently regenerates from the committed protobuf resources while
source transformation and output-hash verification remain unfinished. Set
`GITHUB_TOKEN` when using the resolver in environments subject to GitHub API
rate limits.

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
