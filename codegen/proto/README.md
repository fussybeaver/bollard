# Buildkit rust proto

This repository contains both the protobuf descriptors and generated output.

The generated files are part of the project to maintain consistency across
generated assets in the Bollard project, and to avoid a build dependency on the
external `protoc` binary.

## Fetch

To fetch the protobuf files needed in the Bollard project, this step will fetch
remote files and replace import statements with local equivalents, so that they
can be parsed by prost.

```
cargo run --bin fetch --features fetch
```

## Generate

You will need a protoc compiler for this step. On unix there is usually a
package `protobuf-compiler` or equivalent.

To generate the rust output from the protobuf files use the following:

```
cargo run --bin gen --features build
```

