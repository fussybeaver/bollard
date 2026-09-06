#!/bin/sh
# Regenerate golden LLB protobuf files from the Go parity generator.
# This updates ../../llb/testdata/golden/ in place after verifying that the
# pinned generator produces byte-identical output twice.
set -e
script_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
golden_dir="$repo_root/llb/testdata/golden"
cd "$script_dir"
tmp=$(mktemp -d)
tmp_again=$(mktemp -d)
cleanup() {
  rm -rf "$tmp" "$tmp_again"
}
trap cleanup EXIT
go run . -out "$tmp"
go run . -out "$tmp_again"
if ! diff -ru "$tmp" "$tmp_again"; then
  echo "generator output is not deterministic" >&2
  exit 1
fi
rm -f "$golden_dir"/*.llb.pb "$golden_dir/manifest.json"
cp -a "$tmp"/. "$golden_dir"/
echo "Updated $golden_dir from generator output."
