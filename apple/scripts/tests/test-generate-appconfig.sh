#!/usr/bin/env bash

# Tests apple/scripts/generate-appconfig.sh: the shared app.properties is read
# correctly, values are emitted as valid escaped Swift, and a missing key fails.

set -euo pipefail
unset VNIDROP_DIAGNOSTICS_ENDPOINT VNIDROP_DIAGNOSTICS_INGEST_KEY VNIDROP_REQUIRE_DIAGNOSTICS

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
assert_contains "$scratch/real/AppConfig.swift" 'static let diagnosticsEndpoint = ""'
assert_contains "$scratch/real/AppConfig.swift" 'static let diagnosticsIngestKey = ""'

printf 'PRIVACY_POLICY_URL=https://example.test/privacy/\n' > "$scratch/app.properties"
export VNIDROP_REQUIRE_DIAGNOSTICS=1
expect_failure generate
export VNIDROP_DIAGNOSTICS_ENDPOINT=https://reports.example.test
expect_failure generate
export VNIDROP_DIAGNOSTICS_INGEST_KEY='test-"key"\value'
generate
assert_contains "$out" 'static let diagnosticsEndpoint = "https://reports.example.test"'
assert_contains "$out" 'static let diagnosticsIngestKey = "test-\"key\"\\value"'

# Newer Bash defaults to ASCII glob ranges, which hides the macOS Bash 3.2 failure.
printf 'shopt -u globasciiranges 2>/dev/null || true\n' > "$scratch/bash-env"
for locale in C en_US.UTF-8; do
	LC_ALL="$locale" BASH_ENV="$scratch/bash-env" VNIDROP_DIAGNOSTICS_ENDPOINT=https://diagnostics.example.test \
		VNIDROP_DIAGNOSTICS_INGEST_KEY=apple-ci-fixture generate
	assert_contains "$out" 'static let diagnosticsEndpoint = "https://diagnostics.example.test"'
	assert_contains "$out" 'static let diagnosticsIngestKey = "apple-ci-fixture"'
	LC_ALL="$locale" BASH_ENV="$scratch/bash-env" VNIDROP_DIAGNOSTICS_INGEST_KEY='clé' expect_failure generate
done

for endpoint in 'http://reports.example.test' 'https://' 'https://user@reports.example.test' \
	'https://reports.example.test?key=value' 'https://reports.example.test/#fragment' \
	$'https://reports.example.test/\ninvalid' 'https://reports.example.test:bad'; do
	export VNIDROP_DIAGNOSTICS_ENDPOINT="$endpoint"
	expect_failure generate
done
export VNIDROP_DIAGNOSTICS_ENDPOINT=https://reports.example.test/prefix/
for key in '' ' ' $'key\nheader' $'key\rheader' $'key\theader' $'key\x7f' 'clé'; do
	export VNIDROP_DIAGNOSTICS_INGEST_KEY="$key"
	expect_failure generate
done
export VNIDROP_DIAGNOSTICS_INGEST_KEY=fixture
generate
assert_contains "$out" 'https://reports.example.test/prefix/'

# Partial configuration must also fail outside official release builds.
unset VNIDROP_REQUIRE_DIAGNOSTICS VNIDROP_DIAGNOSTICS_INGEST_KEY
expect_failure generate

# A failed generation leaves the last valid output intact.
assert_contains "$out" 'static let diagnosticsIngestKey = "fixture"'

printf 'generate-appconfig tests passed.\n'
