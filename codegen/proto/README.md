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
and replaces the checked-in resources only after preparation succeeds. It also
writes the complete provenance lock atomically after the resource and generated
output replacements succeed.

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

For development-only Moby source investigations, a mutable Moby branch can be
selected explicitly:

```bash
cargo xtask buildkit update --allow-moby-branch
```

This mode is warned as mutable and must not be used to prepare release
provenance. The default update command remains tag-only.

The generated files remain checked in so Bollard consumers do not need a
generator or `protoc` installation. The unpublished xtask uses the pinned
vendored `protoc` and records the generation contract in the provenance lock.
The pinned versions are `protoc` 31.1, `tonic-prost-build` 0.14.6, and
`prost-build` 0.14.4. Do not set `PROTOC` or `PROTOC_INCLUDE` while running
xtask.
