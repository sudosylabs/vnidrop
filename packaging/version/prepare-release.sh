#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
resolver="$script_dir/resolve-version.sh"
version_file="${VNIDROP_VERSION_FILE:-$repo_root/version.properties}"
next_version="${1:-}"

fail() {
	printf '%s\n' "$*" >&2
	exit 1
}

[[ -n $next_version ]] ||
	fail "Usage: $0 MAJOR.MINOR.PATCH"
[[ -f $version_file ]] ||
	fail "Version file not found: $version_file"
[[ $(grep -c '^PRODUCT_VERSION=' "$version_file") == 1 ]] ||
	fail "Expected exactly one PRODUCT_VERSION entry in $version_file"

current_version="$(
	VNIDROP_VERSION_FILE="$version_file" "$resolver" product
)"
current_android_code="$(
	VNIDROP_VERSION_FILE="$version_file" "$resolver" android-code
)"
temporary="$(mktemp "$(dirname "$version_file")/.version.properties.XXXXXX")"
trap 'rm -f "$temporary"' EXIT

sed "s/^PRODUCT_VERSION=.*/PRODUCT_VERSION=$next_version/" \
	"$version_file" > "$temporary"

next_android_code="$(
	VNIDROP_VERSION_FILE="$temporary" "$resolver" android-code
)"
next_windows_package="$(
	VNIDROP_VERSION_FILE="$temporary" "$resolver" windows-package
)"
VNIDROP_VERSION_FILE="$temporary" "$resolver" verify >/dev/null

(( next_android_code > current_android_code )) ||
	fail "New version must be greater than $current_version"

chmod 644 "$temporary"
mv "$temporary" "$version_file"
trap - EXIT

printf 'Prepared VniDrop %s\n' "$next_version"
printf '  Android version code: %s\n' "$next_android_code"
printf '  Microsoft Store package: %s\n' "$next_windows_package"
printf 'Next: make check-version\n'
