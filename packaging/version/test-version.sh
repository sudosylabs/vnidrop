#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
resolver="$script_dir/resolve-version.sh"
prepare_release="$script_dir/prepare-release.sh"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

write_version() {
	printf '%s\n' \
		"PRODUCT_VERSION=$1" \
		"RELEASE_CHANNEL=$2" \
		"WINDOWS_VERSION_EPOCH=$3" \
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

export VNIDROP_BUILD_TIME_UTC=20260728143217

write_version 0.2.0 beta 1
[[ $(resolve product) == 0.2.0 ]]
[[ $(resolve android-code) == 2000 ]]
[[ $(resolve apple-store-build) == 20260728.1432.17 ]]
[[ $(resolve apple-direct-build) == 20260728.1432.17 ]]
[[ $(resolve windows-package) == 1.2.0.0 ]]
resolve verify-tag v0.2.0
expect_failure resolve verify-tag v1.0.0

write_version 1.0.0 stable 1
[[ $(resolve android-code) == 1000000 ]]
[[ $(resolve windows-package) == 2.0.0.0 ]]

write_version 2099.999.999 stable 1
[[ $(resolve android-code) == 2099999999 ]]

write_version 01.0.0 beta 1
expect_failure resolve verify

write_version 0.0.0 beta 1
expect_failure resolve verify

write_version 2100.0.0 stable 1
expect_failure resolve verify

write_version 0.1000.0 stable 1
expect_failure resolve verify

write_version 0.0.1000 stable 1
expect_failure resolve verify

VNIDROP_BUILD_TIME_UTC=20260728146000 expect_failure resolve verify
VNIDROP_BUILD_TIME_UTC=2026-07-28 expect_failure resolve verify
VNIDROP_BUILD_TIME_UTC=20260229080000 expect_failure resolve verify

write_version 0.2.0 beta 1
config_dir="$scratch/xcconfig"
VNIDROP_APPLE_XCCONFIG_DIR="$config_dir" \
	"$script_dir/generate-apple-xcconfig.sh" all
grep -Fx "PRODUCT_VERSION = 0.2.0" "$config_dir/StoreVersion.xcconfig" >/dev/null
grep -Fx "CURRENT_PROJECT_VERSION = 20260728.1432.17" \
	"$config_dir/StoreVersion.xcconfig" >/dev/null
grep -Fx "CURRENT_PROJECT_VERSION = 20260728.1432.17" \
	"$config_dir/DirectVersion.xcconfig" >/dev/null

VNIDROP_VERSION_FILE="$scratch/version.properties" \
	"$prepare_release" 0.2.1 >/dev/null
[[ $(resolve product) == 0.2.1 ]]
[[ $(resolve android-code) == 2001 ]]
[[ $(resolve windows-package) == 1.2.1.0 ]]
expect_failure env VNIDROP_VERSION_FILE="$scratch/version.properties" \
	"$prepare_release" 0.2.1
expect_failure env VNIDROP_VERSION_FILE="$scratch/version.properties" \
	"$prepare_release" 0.1.999

printf 'Version resolver tests passed.\n'
