#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
resolver="$repo_root/packaging/version/resolve-version.sh"
output_dir="$repo_root/build/release/android"
required_apk_libraries=(
	"lib/arm64-v8a/libvnidrop.so"
	"lib/x86_64/libvnidrop.so"
)
required_aab_libraries=(
	"base/lib/arm64-v8a/libvnidrop.so"
	"base/lib/x86_64/libvnidrop.so"
)

require_environment() {
	local name=$1
	[[ -n ${!name:-} ]] || {
		printf 'Missing required environment variable: %s\n' "$name" >&2
		exit 1
	}
}

normalize_fingerprint() {
	printf '%s' "$1" | tr -d '[:space:]:' | tr '[:upper:]' '[:lower:]'
}

sha256_file() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1" | awk '{print $1}'
	else
		shasum -a 256 "$1" | awk '{print $1}'
	fi
}

verify_archive_entries() {
	local archive=$1
	shift
	local entry
	local size
	for entry in "$@"; do
		size="$(
			unzip -l "$archive" "$entry" |
				awk -v expected="$entry" '$4 == expected {print $1; exit}'
		)"
		[[ -n $size && $size -gt 0 ]] || {
			printf 'Missing or empty Android native library %s in %s\n' \
				"$entry" "$archive" >&2
			exit 1
		}
	done
}

for name in \
	VNIDROP_ANDROID_KEYSTORE_PATH \
	VNIDROP_ANDROID_KEYSTORE_PASSWORD \
	VNIDROP_ANDROID_KEY_ALIAS \
	VNIDROP_ANDROID_KEY_PASSWORD \
	VNIDROP_ANDROID_UPLOAD_CERT_SHA256; do
	require_environment "$name"
done

[[ -r $VNIDROP_ANDROID_KEYSTORE_PATH ]] || {
	printf 'Android upload keystore is not readable: %s\n' "$VNIDROP_ANDROID_KEYSTORE_PATH" >&2
	exit 1
}

version="$("$resolver" product)"
version_code="$("$resolver" android-code)"
"$resolver" verify >/dev/null

cd "$repo_root"
./gradlew \
	:androidApp:check \
	:androidApp:assembleRelease \
	:androidApp:bundleRelease \
	-Pvnidrop.diagnostics.included=false \
	--no-daemon \
	--no-configuration-cache \
	--stacktrace

source_apk="$repo_root/androidApp/build/outputs/apk/release/androidApp-release.apk"
source_aab="$repo_root/androidApp/build/outputs/bundle/release/androidApp-release.aab"
metadata="$repo_root/androidApp/build/intermediates/merged_manifests/release/processReleaseManifest/output-metadata.json"
[[ -s $source_apk && -s $source_aab && -s $metadata ]] || {
	printf 'Android release outputs are missing or empty\n' >&2
	exit 1
}

actual_version="$(jq -r '.elements[0].versionName' "$metadata")"
actual_version_code="$(jq -r '.elements[0].versionCode' "$metadata")"
[[ $actual_version == "$version" && $actual_version_code == "$version_code" ]] || {
	printf 'Android artifact version mismatch: expected %s (%s), got %s (%s)\n' \
		"$version" "$version_code" "$actual_version" "$actual_version_code" >&2
	exit 1
}

jarsigner_report="$(jarsigner -verify "$source_aab" 2>&1)" || {
	printf 'AAB signature verification failed:\n%s\n' "$jarsigner_report" >&2
	exit 1
}
grep -F 'jar verified.' <<< "$jarsigner_report" >/dev/null || {
	printf 'jarsigner did not confirm the AAB signature\n' >&2
	exit 1
}
verify_archive_entries "$source_apk" "${required_apk_libraries[@]}"
verify_archive_entries "$source_aab" "${required_aab_libraries[@]}"
actual_fingerprint="$(
	"$script_dir/verify-apk-signature.sh" \
		"$source_apk" \
		"$VNIDROP_ANDROID_UPLOAD_CERT_SHA256"
)"
expected_fingerprint="$(normalize_fingerprint "$VNIDROP_ANDROID_UPLOAD_CERT_SHA256")"
aab_fingerprint="$(
	keytool -printcert -jarfile "$source_aab" |
		awk -F': ' '/SHA256:/ {print $2; exit}'
)"
aab_fingerprint="$(normalize_fingerprint "$aab_fingerprint")"
[[ $aab_fingerprint == "$expected_fingerprint" ]] || {
	printf 'AAB signing certificate mismatch: expected %s, got %s\n' \
		"$expected_fingerprint" "$aab_fingerprint" >&2
	exit 1
}

mkdir -p "$output_dir"
rm -f \
	"$output_dir"/VniDrop-*-upload-signed.apk \
	"$output_dir"/VniDrop-*.aab \
	"$output_dir"/SHA256SUMS
apk_name="VniDrop-${version}-${version_code}-upload-signed.apk"
aab_name="VniDrop-${version}-${version_code}.aab"
cp "$source_apk" "$output_dir/$apk_name"
cp "$source_aab" "$output_dir/$aab_name"

{
	printf '%s  %s\n' "$(sha256_file "$output_dir/$apk_name")" "$apk_name"
	printf '%s  %s\n' "$(sha256_file "$output_dir/$aab_name")" "$aab_name"
} > "$output_dir/SHA256SUMS"

printf 'Created signed Android release artifacts:\n'
printf '  %s\n' "$output_dir/$aab_name"
printf '  %s\n' "$output_dir/$apk_name"
printf '  upload certificate SHA-256: %s\n' "$actual_fingerprint"
