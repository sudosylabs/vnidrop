#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
	printf 'Usage: %s <apk> <expected-certificate-sha256>\n' "$0" >&2
	exit 2
fi

apk=$1
expected_fingerprint=$2
build_tools_version=${ANDROID_BUILD_TOOLS_VERSION:-36.0.0}

normalize_fingerprint() {
	printf '%s' "$1" |
		tr -d '[:space:]:' |
		tr '[:upper:]' '[:lower:]'
}

find_apksigner() {
	if [[ -n ${APKSIGNER:-} ]]; then
		[[ -x $APKSIGNER ]] || {
			printf 'Configured apksigner is not executable: %s\n' "$APKSIGNER" >&2
			return 1
		}
		printf '%s\n' "$APKSIGNER"
		return
	fi

	local sdk_root=${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}
	if [[ -n $sdk_root ]]; then
		local pinned="$sdk_root/build-tools/$build_tools_version/apksigner"
		if [[ -x $pinned ]]; then
			printf '%s\n' "$pinned"
			return
		fi
	fi

	if command -v apksigner >/dev/null 2>&1; then
		command -v apksigner
		return
	fi

	printf 'apksigner %s was not found in the Android SDK or PATH\n' \
		"$build_tools_version" >&2
	return 1
}

[[ -s $apk ]] || {
	printf 'APK is missing or empty: %s\n' "$apk" >&2
	exit 1
}

apksigner_path="$(find_apksigner)" || exit 1
if ! signature_report="$(
	"$apksigner_path" verify --verbose --print-certs "$apk" 2>&1
)"; then
	printf 'APK signature verification failed:\n%s\n' "$signature_report" >&2
	exit 1
fi

actual_fingerprint="$(
	printf '%s\n' "$signature_report" |
		awk '
			tolower($0) ~ /^signer #1 certificate sha-256 digest:[[:space:]]*/ {
				line = $0
				sub(/^[^:]*:[[:space:]]*/, "", line)
				print line
				exit
			}
		'
)"
[[ -n $actual_fingerprint ]] || {
	printf 'Could not read the APK signing certificate fingerprint\n' >&2
	exit 1
}

actual_fingerprint="$(normalize_fingerprint "$actual_fingerprint")"
expected_fingerprint="$(normalize_fingerprint "$expected_fingerprint")"
[[ $actual_fingerprint == "$expected_fingerprint" ]] || {
	printf 'APK signing certificate mismatch: expected %s, got %s\n' \
		"$expected_fingerprint" "$actual_fingerprint" >&2
	exit 1
}

printf '%s\n' "$actual_fingerprint"
