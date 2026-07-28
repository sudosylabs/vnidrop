#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
resolver="$script_dir/resolve-version.sh"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

write_version() {
	printf '%s\n' \
		"PRODUCT_VERSION=$1" \
		"RELEASE_CHANNEL=$2" \
		"ANDROID_VERSION_CODE=$3" \
		"APPLE_BUILD_NUMBER=$4" \
		"WINDOWS_VERSION_EPOCH=$5" \
		> "$scratch/version.properties"
}

resolve() {
	VNIDROP_VERSION_FILE="$scratch/version.properties" "$resolver" "$@"
}

expect_failure() {
	if "$@" >/dev/null 2>&1; then
		printf 'Expected command to fail: %s\n' "$*" >&2
		exit 1
	fi
}

write_version 0.2.0 beta 2 2 1
[[ $(resolve product) == 0.2.0 ]]
[[ $(resolve android-code) == 2 ]]
[[ $(resolve apple-build) == 2 ]]
[[ $(resolve windows-package) == 1.2.0.0 ]]
resolve verify-tag v0.2.0
expect_failure resolve verify-tag v1.0.0

write_version 1.0.0 stable 42 42 1
[[ $(resolve windows-package) == 2.0.0.0 ]]

write_version 01.0.0 beta 2 2 1
expect_failure resolve verify

write_version 0.2.0 beta 0 2 1
expect_failure resolve verify

write_version 65535.0.0 stable 2 2 1
expect_failure resolve verify

printf 'Version resolver tests passed.\n'
