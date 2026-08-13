# Bollard Release Process

This document outlines the steps for releasing a new version of Bollard. Follow these instructions carefully to ensure a smooth release process.

## 1. Update and Publish Protobuf-Generated Files

### Steps for `bollard-buildkit-proto`

The preferred workflow is run from the repository root. The commands must be run
in this order:

```sh
# 1. Select the Moby release in codegen/swagger/pom.xml.
# 2. Derive and stage the complete provenance-aligned update:
cargo xtask buildkit update
# 3. Validate the staged protobuf, Go oracle, generated goldens, and Rust output.
cargo xtask buildkit check
# 4. Re-fetch immutable upstream sources for release preparation.
cargo xtask buildkit check --online
```

`update` derives the immutable Moby and BuildKit compatibility baseline from
`codegen/swagger/pom.xml`, fetches the dependency-classified protobuf sources,
applies named transformations with exact match counts, updates the exact Go
BuildKit oracle requirement, runs `go mod tidy`, regenerates the LLB goldens
twice, and stages source and transformed-output hashes. It replaces the
checked-in protobuf resources, generated Rust, Go oracle, golden fixtures, and
provenance lock only after every preparation and determinism check succeeds.
`check` enforces the complete lock, oracle, manifest, and generated-output set
without network access. `check --online` re-fetches and verifies immutable
upstream source hashes after the offline check passes. The xtask uses the
vendored `protoc` and exact generator versions recorded in the provenance lock;
do not set `PROTOC` or `PROTOC_INCLUDE` while running it.

Routine pull requests must use the offline `check`; it must not depend on
GitHub availability. Release preparation must run both `check` and
`check --online`, with the online check refetching every immutable source and
verifying its source hash. Generator dependency updates must retain the exact
xtask requirements, review the bundled `protoc --version`, regenerate and
review output, pass `cargo audit`, and pass the full provenance and test gates.

Review and validation chronology after `update`:

1. Review the Moby/BuildKit provenance lock, protobuf resources, generated Rust,
   `codegen/llb-parity/go.mod`, `go.sum`, golden manifest, and fixture diffs.
2. Check for transient dependency updates between `bollard` and
   `bollard-buildkit-proto` (e.g., `tonic`).
3. Run both provenance checks and review the lock, resource, oracle, and golden
   diffs:
   ```sh
   cargo xtask buildkit check
   cargo xtask buildkit check --online
   ```
4. Run the Go oracle checks:
   ```sh
   (cd codegen/llb-parity && go test ./... && go vet ./...)
   ```
5. Verify that the build and compatibility suites succeed:
     - In the project root, attempt a build with the `buildkit` feature enabled.
     - Run the LLB parity, mutation, platform, solve, and export tests.
     - Confirm the live BuildKit record reports the requested image, resolved
       image ID and repository digests, daemon tool versions, Go oracle version,
       `ops.proto` hash, and generated provenance values.
     - Use `BOLLARD_BUILDKIT_TEST_IMAGE` only for a separate compatibility run;
       it must not regenerate schemas or golden fixtures.
     - Run semver checks and review any generated public API changes.
    - Temporarily add a path dependency in `Cargo.toml` and update related transient dependencies to test the changes.
    - **Important:** Revert any path dependency and ensure version alignment in `Cargo.toml` before submitting a pull request.
6. Package and publish the crate:
   ```sh
   cargo package
   cargo publish
   ```
7. Create a PR and merge the changes.

For the extraction release, publish in dependency order: `bollard-buildkit-proto`,
matching `bollard-stubs` when required, `bollard-llb`, and finally Bollard.
Mutable Moby or BuildKit branch investigations are development-only and must
not prepare release provenance.

## 2. Update and Publish Swagger-Generated Files

### Steps for `bollard-swagger`

1. Navigate to `./codegen/swagger`.
2. Check for transient dependency updates between `bollard` and `bollard-buildkit-proto` (e.g., `chrono`).
3. Identify the latest released API version:
   - Check the latest release tag on the [Moby GitHub repository](https://github.com/moby/moby/releases/).
   - Locate the most recent API documentation in `./docs/api`.
   - Copy the raw download URL and update `./codegen/swagger/pom.xml` accordingly.
4. Update the `packageVersion` field:
   - The first two numbers represent the Moby API version.
   - The third number corresponds to Bollard's internal codegen version.
   - The digits following `rc` match the Moby release tag.
   - Format: `[API-major].[API-minor].[bollard-codegen-version]-rc.[moby-tag-major][moby-tag-minor].[moby-tag-patch]`.
5. Modify `Cargo.mustache` to reference the new `bollard-buildkit-proto` version.
6. Generate the new Swagger bindings:
   ```sh
   mvn -D org.slf4j.simpleLogger.defaultLogLevel=warn clean compiler:compile generate-resources
   ```
7. Validate the build:
   - Run a build in the root directory.
   - Temporarily add a path dependency in `Cargo.toml` to verify correctness.
8. Merge the pull request and reset the `master` branch.
9. Package and publish the crate:
   ```sh
   cargo package
   cargo publish
   ```
10. Create a PR and merge the changes.

## 3. Update Bollard Crate and Documentation

1. Update `Cargo.toml`:
   - Modify dependencies to point to the latest versions of `bollard-buildkit-proto` and `bollard-stubs`.
2. Update `lib.rs` and `docker.rs` to match the new API version if changes were made in the Swagger release.
3. Modify `lib.rs` with any relevant documentation updates.
4. Regenerate the README:
   ```sh
   cargo readme --no-title > README.md
   ```

## 4. Publish the New Release

1. Bump the crate version as necessary.
2. Package and publish the release:
   ```sh
   cargo package
   cargo publish
   ```
3. Create a GitHub Release tag (this should be autogenerated).

Following these steps ensures consistency and reliability when publishing new versions of Bollard. If any issues arise, review the steps carefully or consult the project maintainers for guidance.
