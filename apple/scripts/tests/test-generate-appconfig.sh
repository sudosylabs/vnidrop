#!/usr/bin/env bash

# Tests apple/scripts/generate-appconfig.sh: the shared app.properties is read
# correctly, values are emitted as valid escaped Swift, and a missing key fails.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
generator="$script_dir/../generate-appconfig.sh"
repo_root="$(cd "$script_dir/../../.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# Run the generator against a fixture app.properties, emitting into a temp dir.
generate() {
	VNIDROP_APP_PROPERTIES="$scratch/app.properties" \
		VNIDROP_APPLE_GENERATED_DIR="$scratch/out" \
		"$generator"
}

expect_failure() {
	if "$@" >/dev/null 2>&1; then
		printf 'Expected command to fail: %s\n' "$*" >&2
		exit 1
	fi
}

assert_contains() {
	local file=$1 needle=$2
	grep -qF "$needle" "$file" ||
		{ printf 'Expected %s to contain: %s\n' "$file" "$needle" >&2; exit 1; }
}

out="$scratch/out/AppConfig.swift"

# 1. Nominal value is emitted verbatim as a Swift URL literal.
printf 'PRIVACY_POLICY_URL=%s\n' 'https://example.test/privacy/' > "$scratch/app.properties"
generate
assert_contains "$out" 'URL(string: "https://example.test/privacy/")!'
assert_contains "$out" 'enum AppConfig'

# 2. Characters special to a Swift string literal are escaped.
printf 'PRIVACY_POLICY_URL=%s\n' 'https://a.test/"q"\z' > "$scratch/app.properties"
generate
assert_contains "$out" 'URL(string: "https://a.test/\"q\"\\z")!'

# 3. A missing key fails instead of emitting an empty value.
printf 'OTHER_KEY=value\n' > "$scratch/app.properties"
expect_failure generate

# 4. A duplicated key fails.
printf 'PRIVACY_POLICY_URL=a\nPRIVACY_POLICY_URL=b\n' > "$scratch/app.properties"
expect_failure generate

# 5. The real committed app.properties produces an https URL.
VNIDROP_APPLE_GENERATED_DIR="$scratch/real" "$generator"
assert_contains "$scratch/real/AppConfig.swift" 'URL(string: "https://'

printf 'generate-appconfig tests passed.\n'
