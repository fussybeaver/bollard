# bollard-llb

`bollard-llb` constructs synchronous BuildKit Low-Level Builder (LLB)
definitions. It emits `bollard-buildkit-proto::pb::Definition` values and can
write them as binary protobuf for `buildctl`.

The current BuildKit compatibility baseline is recorded in the repository's
[provenance lock](https://github.com/fussybeaver/bollard/blob/master/codegen/proto/provenance.lock.toml).

## Supported Graph Surface

- Scratch and image sources with platform, resolve-mode, checksum and layer-limit options.
- Local sources with include, exclude, follow-path, session and shared-key options.
- Exec arguments, shell splitting, environment variables and working directories.
- Sandbox, host and none networking; sandbox and insecure security modes.
- Bind, scratch and cache mounts.
- File and environment secrets.
- SSH agent sockets with named IDs, targets, permissions and optional providers.
- Copy, mkdir, mkfile, remove and symlink file actions.
- Merge operations, worker filters, metadata, deterministic serialization and text/JSON dumps.

SSH forwarding is Unix-only for the first release. Empty SSH IDs select the
BuildKit `default` provider. Direct solves must register a provider with
Bollard's solve driver; an unavailable required provider is rejected, while an
optional provider is allowed to remain unavailable.

## Direct Solves

This crate only constructs graphs. Bollard's direct-definition solve API
currently supports the `DockerContainer` and `BuildkitDaemon` drivers with the
local exporter. A caller must explicitly grant `network.host` and
`security.insecure` entitlements to a direct solve and, for a Docker-container
daemon, configure the matching daemon allowance.

Direct solves own their session ID. Do not add a `local.session` attribute to a
definition submitted through that API. Local sources without that attribute use
the active solve session.

## Dependency Boundary

The graph builder is synchronous and does not use a network transport. With
default features disabled, it resolves `bollard-buildkit-proto` without its RPC
feature and does not pull `tokio`, `tonic`, TLS or an HTTP client stack.

The following features remain outside the initial release and are tracked for
later compatible expansion: Git and HTTP sources, OCI-layout and image-blob
sources, Diff and CDI operations, tmpfs and richer mount controls, richer exec
metadata, multi-action file operations, richer exporters, gateway/frontend APIs
and progress streaming.
