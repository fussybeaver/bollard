#!/bin/sh
# Run BuildKit's parser and the pinned Go client against one committed golden.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)

if [ "$#" -ne 3 ]; then
	printf 'usage: %s <buildkit-container> <fixture.llb.pb> <output-dir>\n' "$0" >&2
	exit 2
fi

container=$1
fixture=$2
output=$3
remote=/tmp/bollard-llb-parity-$(basename "$fixture")
fixture=$(CDPATH= cd -- "$(dirname "$fixture")" && pwd)/$(basename "$fixture")
output=$(CDPATH= cd -- "$(dirname "$output")" && pwd)/$(basename "$output")
trap 'docker exec "$container" rm -f "$remote" >/dev/null 2>&1 || true' EXIT

docker cp "$fixture" "$container:$remote"
printf '%s\n' '=== buildctl debug dump-llb ==='
docker exec "$container" buildctl debug dump-llb "$remote"

printf '%s\n' '=== BuildKit Go client solve ==='
(
    cd "$script_dir"
    go run ./independent_solve \
	-address "docker-container://$container" \
	-fixture "$fixture" \
	-output "$output"
)
