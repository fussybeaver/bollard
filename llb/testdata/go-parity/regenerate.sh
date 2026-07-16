#!/bin/sh
# Regenerate golden LLB protobuf files from the Go parity generator.
# This updates ../golden/ in place and verifies the output with a fresh
# generation before copying.
set -e
cd "$(dirname "$0")"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
go run . -out "$tmp"
go run . -check ../golden -against "$tmp"
cp -a "$tmp"/* ../golden/
echo "Updated ../golden from generator output."