#!/bin/sh
# Regenerate golden LLB protobuf files from the Go parity generator.
# This updates ../../llb/testdata/golden/ in place and verifies the output with a fresh
# generation before copying.
set -e
script_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
golden_dir="$repo_root/llb/testdata/golden"
cd "$script_dir"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
go run . -out "$tmp"
go run . -check "$golden_dir" -against "$tmp"
cp -a "$tmp"/* "$golden_dir"/
echo "Updated $golden_dir from generator output."
