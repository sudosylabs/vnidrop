#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
resolver="$repo_root/packaging/version/resolve-version.sh"
output_dir="$repo_root/build/release/android"
mode="${1:-release}"
case "$mode" in
	release) ;;
	preview) output_dir="$repo_root/build/preview/android" ;;
	*) echo 'Usage: build-release.sh [release|preview]' >&2; exit 1 ;;
esac
required_apk_libraries=(
	"lib/arm64-v8a/libvnidrop.so"
	"lib/x86_64/libvnidrop.so"
	"lib/arm64-v8a/libzxingcpp_android.so"
	"lib/x86_64/libzxingcpp_android.so"
)
required_aab_libraries=(
	"base/lib/arm64-v8a/libvnidrop.so"
	"base/lib/x86_64/libvnidrop.so"
	"base/lib/arm64-v8a/libzxingcpp_android.so"
	"base/lib/x86_64/libzxingcpp_android.so"
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
		if ! size="$(
			unzip -l "$archive" "$entry" |
				awk -v expected="$entry" '$4 == expected {print $1; exit}'
		)" || [[ -z $size || $size -le 0 ]]; then
			printf 'Missing or empty Android native library %s in %s\n' \
				"$entry" "$archive" >&2
			exit 1
		fi
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
	printf 'Android signing keystore is not readable: %s\n' "$VNIDROP_ANDROID_KEYSTORE_PATH" >&2
	exit 1
}

version="$("$resolver" product)"
version_code="$("$resolver" android-code)"
"$resolver" verify >/dev/null

tasks=(:androidApp:lintRelease :androidApp:assembleRelease)
properties=()
application_id=com.vnidrop.app
if [[ $mode == preview ]]; then
	require_environment VNIDROP_PREVIEW_NUMBER
	[[ $VNIDROP_PREVIEW_NUMBER =~ ^[1-9][0-9]{0,9}$ ]] &&
		(( VNIDROP_PREVIEW_NUMBER <= 2100000000 )) || {
		echo 'Preview number must be between 1 and 2100000000' >&2
		exit 1
	}
	version="$version-preview.$VNIDROP_PREVIEW_NUMBER"
	version_code="$VNIDROP_PREVIEW_NUMBER"
	application_id=com.vnidrop.app.preview
	properties+=("-Pvnidrop.preview.number=$VNIDROP_PREVIEW_NUMBER")
else
	tasks+=(:androidApp:bundleRelease)
fi

cd "$repo_root"
./gradlew \
	"${tasks[@]}" "${properties[@]}" \
	-Pvnidrop.diagnostics.included=true \
	--no-daemon \
	--no-configuration-cache \
	--stacktrace

source_apk="$repo_root/androidApp/build/outputs/apk/release/androidApp-release.apk"
source_aab="$repo_root/androidApp/build/outputs/bundle/release/androidApp-release.aab"
metadata="$repo_root/androidApp/build/intermediates/merged_manifests/release/processReleaseManifest/output-metadata.json"
[[ -s $source_apk && -s $metadata ]] || {
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

if [[ $mode == preview ]]; then
	apkanalyzer="${APKANALYZER:-${ANDROID_HOME:?Android SDK is required}/cmdline-tools/latest/bin/apkanalyzer}"
	[[ $("$apkanalyzer" manifest application-id "$source_apk") == "$application_id" ]]
	[[ $("$apkanalyzer" manifest debuggable "$source_apk") == false ]]
	[[ $("$apkanalyzer" manifest version-name "$source_apk") == "$version" ]]
	[[ $("$apkanalyzer" manifest version-code "$source_apk") == "$version_code" ]]
fi

if [[ $mode == release ]]; then
	[[ -s $source_aab ]] || { echo 'Android release AAB is missing or empty' >&2; exit 1; }
	jarsigner_report="$(jarsigner -verify "$source_aab" 2>&1)" || {
		printf 'AAB signature verification failed:\n%s\n' "$jarsigner_report" >&2
		exit 1
	}
	grep -F 'jar verified.' <<< "$jarsigner_report" >/dev/null || {
		printf 'jarsigner did not confirm the AAB signature\n' >&2
		exit 1
	}
	verify_archive_entries "$source_aab" "${required_aab_libraries[@]}"
fi
verify_archive_entries "$source_apk" "${required_apk_libraries[@]}"
actual_fingerprint="$(
	"$script_dir/verify-apk-signature.sh" \
		"$source_apk" \
		"$VNIDROP_ANDROID_UPLOAD_CERT_SHA256"
)"
expected_fingerprint="$(normalize_fingerprint "$VNIDROP_ANDROID_UPLOAD_CERT_SHA256")"
if [[ $mode == release ]]; then
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
fi

mkdir -p "$output_dir"
rm -f \
	"$output_dir"/VniDrop-*-upload-signed.apk \
	"$output_dir"/VniDrop-*-preview.*.apk \
	"$output_dir"/VniDrop-*.aab \
	"$output_dir"/SHA256SUMS
apk_name="VniDrop-${version}-${version_code}-upload-signed.apk"
aab_name="VniDrop-${version}-${version_code}.aab"
if [[ $mode == preview ]]; then
	apk_name="VniDrop-${version}.apk"
fi
cp "$source_apk" "$output_dir/$apk_name"
if [[ $mode == release ]]; then
	cp "$source_aab" "$output_dir/$aab_name"
fi

{
	printf '%s  %s\n' "$(sha256_file "$output_dir/$apk_name")" "$apk_name"
	if [[ $mode == release ]]; then
		printf '%s  %s\n' "$(sha256_file "$output_dir/$aab_name")" "$aab_name"
	fi
} > "$output_dir/SHA256SUMS"

if [[ $mode == preview ]]; then
	jq -n --arg applicationId "$application_id" --arg versionName "$version" \
		--argjson versionCode "$version_code" --arg signingCertificateSha256 "$actual_fingerprint" \
		'{applicationId: $applicationId, versionName: $versionName, versionCode: $versionCode,
		  signingCertificateSha256: $signingCertificateSha256, debuggable: false}' \
		> "$output_dir/android-preview.json"
fi

printf 'Created signed Android release artifacts:\n'
[[ $mode == preview ]] || printf '  %s\n' "$output_dir/$aab_name"
printf '  %s\n' "$output_dir/$apk_name"
printf '  signing certificate SHA-256: %s\n' "$actual_fingerprint"
