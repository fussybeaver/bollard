#!/bin/sh
# Independently parse and solve a committed fixture using the pinned BuildKit
# image and BuildKit's Go client, without Bollard's solve or transport code.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
lock_file="$repo_root/codegen/proto/provenance.lock.toml"
fixture="$repo_root/llb/testdata/golden/mkfile.llb.pb"
container="bollard-llb-independent-$$"
temporary_directory=$(mktemp -d)
output="$temporary_directory/output"

lock_value() {
	section=$1
	key=$2
	awk -v section="[$section]" -v key="$key" '
		$0 == section { in_section = 1; next }
		/^\[/ { in_section = 0 }
		in_section && $1 == key && $2 == "=" {
			value = $3
			gsub(/^"|"$/, "", value)
			print value
			exit
		}
	' "$lock_file"
}

image=$(lock_value buildkit image)
expected_version=$(lock_value buildkit version)
expected_commit=$(lock_value buildkit commit)
if [ -z "$image" ] || [ -z "$expected_version" ] || [ -z "$expected_commit" ]; then
	echo "BuildKit image/version/commit missing from $lock_file" >&2
	exit 1
fi

cleanup() {
	docker rm -f -v "$container" >/dev/null 2>&1 || true
	rm -rf "$temporary_directory"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

docker pull "$image"
image_id=$(docker image inspect --format '{{.Id}}' "$image")
echo "independent_solve_image=$image image_id=$image_id"
docker run --detach --privileged --name "$container" "$image"

ready=false
attempt=0
while [ "$attempt" -lt 60 ]; do
	if docker exec "$container" buildctl debug workers >/dev/null 2>&1; then
		ready=true
		break
	fi
	attempt=$((attempt + 1))
	sleep 2
done
if [ "$ready" != true ]; then
	docker logs "$container" >&2 || true
	echo "BuildKit did not become ready within 120 seconds" >&2
	exit 1
fi

buildkitd_version=$(docker exec "$container" buildkitd --version)
buildctl_version=$(docker exec "$container" buildctl --version)
printf '%s\n' "$buildkitd_version" "$buildctl_version"
case "$buildkitd_version" in
	*"$expected_version"*"$expected_commit"*) ;;
	*) echo "buildkitd version does not match provenance lock" >&2; exit 1 ;;
esac
case "$buildctl_version" in
	*"$expected_version"*"$expected_commit"*) ;;
	*) echo "buildctl version does not match provenance lock" >&2; exit 1 ;;
esac

sh "$script_dir/independent_solve.sh" "$container" "$fixture" "$output"

if [ "$(cat "$output/hello")" != world ]; then
	echo "independent solve exported unexpected contents" >&2
	exit 1
fi
mode=$(stat -c '%a' "$output/hello")
if [ "$mode" != 644 ]; then
	echo "independent solve exported mode $mode; expected 644" >&2
	exit 1
fi
echo "independent_solve_export=PASS file=hello contents=world mode=$mode"
