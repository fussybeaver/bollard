#!/bin/sh
# Run the pinned image's BuildKit parser and CLI against one committed golden.
set -eu

if [ "$#" -ne 3 ]; then
	printf 'usage: %s <buildkit-container> <fixture.llb.pb> <output-dir>\n' "$0" >&2
	exit 2
fi

container=$1
fixture=$2
output=$3
fixture=$(CDPATH= cd -- "$(dirname "$fixture")" && pwd)/$(basename "$fixture")
output=$(CDPATH= cd -- "$(dirname "$output")" && pwd)/$(basename "$output")
remote=$(docker exec "$container" mktemp -d /tmp/bollard-llb-parity.XXXXXX)
trap 'docker exec "$container" rm -rf "$remote" >/dev/null 2>&1 || true' EXIT
trap 'exit 1' HUP INT TERM

docker cp "$fixture" "$container:$remote/fixture.llb.pb"
printf '%s\n' '=== buildctl debug dump-llb ==='
docker exec "$container" buildctl debug dump-llb "$remote/fixture.llb.pb"

printf '%s\n' '=== buildctl build (raw LLB) ==='
# No frontend: buildctl reads the committed protobuf definition from stdin.
# Export inside the container so this also works with CircleCI remote Docker.
timeout 120 docker exec -i "$container" buildctl build \
	--output "type=local,dest=$remote/output" < "$fixture"
mkdir -p "$output"
docker cp "$container:$remote/output/." "$output"
