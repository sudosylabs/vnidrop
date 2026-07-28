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
windows_version_epoch="$(read_property WINDOWS_VERSION_EPOCH)"
build_time_utc="${VNIDROP_BUILD_TIME_UTC:-$(date -u +%Y%m%d%H%M%S)}"

[[ $product_version =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
	fail "PRODUCT_VERSION must use canonical MAJOR.MINOR.PATCH integers"
IFS=. read -r product_major product_minor product_patch <<< "$product_version"
validate_canonical_integer "PRODUCT_VERSION major" "$product_major" 0 65534
validate_canonical_integer "PRODUCT_VERSION minor" "$product_minor" 0 65535
validate_canonical_integer "PRODUCT_VERSION patch" "$product_patch" 0 65535
[[ $release_channel =~ ^[a-z][a-z0-9-]*$ ]] ||
	fail "RELEASE_CHANNEL must start with a lowercase letter and contain only lowercase letters, digits, and hyphens"
validate_canonical_integer "ANDROID_VERSION_CODE" "$android_version_code" 1 2100000000
validate_canonical_integer "WINDOWS_VERSION_EPOCH" "$windows_version_epoch" 1 65535
[[ $build_time_utc =~ ^[0-9]{14}$ ]] ||
	fail "VNIDROP_BUILD_TIME_UTC must use YYYYMMDDHHMMSS"

build_month="${build_time_utc:4:2}"
build_day="${build_time_utc:6:2}"
build_hour="${build_time_utc:8:2}"
build_minute="${build_time_utc:10:2}"
build_second="${build_time_utc:12:2}"
build_year="${build_time_utc:0:4}"
(( 10#$build_year >= 1 )) ||
	fail "VNIDROP_BUILD_TIME_UTC contains an invalid year"
(( 10#$build_month >= 1 && 10#$build_month <= 12 )) ||
	fail "VNIDROP_BUILD_TIME_UTC contains an invalid month"

case $((10#$build_month)) in
	2)
		max_build_day=28
		if (( 10#$build_year % 400 == 0 ||
			(10#$build_year % 4 == 0 && 10#$build_year % 100 != 0) )); then
			max_build_day=29
		fi
		;;
	4|6|9|11)
		max_build_day=30
		;;
	*)
		max_build_day=31
		;;
esac
(( 10#$build_day >= 1 && 10#$build_day <= max_build_day )) ||
	fail "VNIDROP_BUILD_TIME_UTC contains an invalid day"
(( 10#$build_hour <= 23 && 10#$build_minute <= 59 && 10#$build_second <= 59 )) ||
	fail "VNIDROP_BUILD_TIME_UTC contains an invalid time"
apple_build_number="${build_time_utc:0:8}.${build_time_utc:8:4}.${build_time_utc:12:2}"

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
	apple-store-build)
		printf '%s\n' "$apple_build_number"
		;;
	apple-direct-build)
		printf '%s\n' "$apple_build_number"
		;;
	windows-package)
		printf '%s\n' "$windows_package_version"
		;;
	verify)
		verify_tag
		printf 'VniDrop %s (%s), Android %s, Apple Store %s, Apple Direct %s, MSIX %s\n' \
			"$product_version" "$release_channel" "$android_version_code" \
			"$apple_build_number" "$apple_build_number" "$windows_package_version"
		;;
	verify-tag)
		verify_tag "${2:-}"
		;;
	*)
		fail "Usage: $0 {product|channel|android-code|apple-store-build|apple-direct-build|windows-package|verify|verify-tag [tag]}"
		;;
esac
