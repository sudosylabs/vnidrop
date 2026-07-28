#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
version_file="${VNIDROP_VERSION_FILE:-$repo_root/version.properties}"

fail() {
	printf '%s\n' "$*" >&2
	exit 1
}

read_property() {
	local key=$1
	local matches
	matches="$(sed -n "s/^${key}=//p" "$version_file")"
	[[ -n "$matches" ]] || fail "Missing $key in $version_file"
	[[ $(printf '%s\n' "$matches" | wc -l | tr -d ' ') == 1 ]] ||
		fail "Duplicate $key in $version_file"
	printf '%s' "$matches"
}

validate_canonical_integer() {
	local name=$1
	local value=$2
	local minimum=$3
	local maximum=$4
	[[ $value =~ ^(0|[1-9][0-9]*)$ ]] ||
		fail "$name must be a canonical non-negative integer"
	(( 10#$value >= minimum && 10#$value <= maximum )) ||
		fail "$name must be between $minimum and $maximum"
}

product_version="$(read_property PRODUCT_VERSION)"
release_channel="$(read_property RELEASE_CHANNEL)"
android_version_code="$(read_property ANDROID_VERSION_CODE)"
apple_build_number="$(read_property APPLE_BUILD_NUMBER)"
windows_version_epoch="$(read_property WINDOWS_VERSION_EPOCH)"

[[ $product_version =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
	fail "PRODUCT_VERSION must use canonical MAJOR.MINOR.PATCH integers"
IFS=. read -r product_major product_minor product_patch <<< "$product_version"
validate_canonical_integer "PRODUCT_VERSION major" "$product_major" 0 65534
validate_canonical_integer "PRODUCT_VERSION minor" "$product_minor" 0 65535
validate_canonical_integer "PRODUCT_VERSION patch" "$product_patch" 0 65535
[[ $release_channel =~ ^[a-z][a-z0-9-]*$ ]] ||
	fail "RELEASE_CHANNEL must start with a lowercase letter and contain only lowercase letters, digits, and hyphens"
validate_canonical_integer "ANDROID_VERSION_CODE" "$android_version_code" 1 2100000000
[[ $apple_build_number =~ ^[1-9][0-9]*(\.[0-9]+){0,2}$ ]] ||
	fail "APPLE_BUILD_NUMBER must contain one to three period-separated non-negative integers and start above zero"
validate_canonical_integer "WINDOWS_VERSION_EPOCH" "$windows_version_epoch" 1 65535

windows_major=$((10#$product_major + 10#$windows_version_epoch))
(( windows_major <= 65535 )) ||
	fail "Derived Windows package major exceeds 65535"
windows_package_version="$windows_major.$product_minor.$product_patch.0"

verify_tag() {
	local tag=${1:-${GITHUB_REF_NAME:-}}
	if [[ ${GITHUB_REF_TYPE:-} == tag || -n ${1:-} ]]; then
		[[ $tag == "v$product_version" ]] ||
			fail "Release tag must be v$product_version, got ${tag:-<empty>}"
	fi
}

case "${1:-verify}" in
	product)
		printf '%s\n' "$product_version"
		;;
	channel)
		printf '%s\n' "$release_channel"
		;;
	android-code)
		printf '%s\n' "$android_version_code"
		;;
	apple-build)
		printf '%s\n' "$apple_build_number"
		;;
	windows-package)
		printf '%s\n' "$windows_package_version"
		;;
	verify)
		verify_tag
		printf 'VniDrop %s (%s), Android %s, Apple %s, MSIX %s\n' \
			"$product_version" "$release_channel" "$android_version_code" \
			"$apple_build_number" "$windows_package_version"
		;;
	verify-tag)
		verify_tag "${2:-}"
		;;
	*)
		fail "Usage: $0 {product|channel|android-code|apple-build|windows-package|verify|verify-tag [tag]}"
		;;
esac
