#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
resolver="$repo_root/packaging/version/resolve-version.sh"
input_dir="${VNIDROP_RELEASE_INPUT_DIR:-$repo_root/build/release/downloads}"
output_dir="${VNIDROP_RELEASE_OUTPUT_DIR:-$repo_root/build/release/final}"
source_commit="${GITHUB_SHA:-local}"
source_tag="${GITHUB_REF_NAME:-v$("$resolver" product)}"

find_single() {
	local directory=$1
	local pattern=$2
	local label=$3
	local matches=()
	local match
	while IFS= read -r match; do
		matches+=("$match")
	done < <(find "$directory" -type f -name "$pattern" -print)
	[[ ${#matches[@]} == 1 ]] || {
		printf 'Expected exactly one %s under %s, found %s\n' \
			"$label" "$directory" "${#matches[@]}" >&2
		exit 1
	}
	printf '%s' "${matches[0]}"
}

file_size() {
	if stat -c '%s' "$1" >/dev/null 2>&1; then
		stat -c '%s' "$1"
	else
		stat -f '%z' "$1"
	fi
}

verify_checksum_file() {
	local checksum_file=$1
	(
		cd "$(dirname "$checksum_file")"
		sha256sum --check "$(basename "$checksum_file")"
	)
}

version="$("$resolver" product)"
android_code="$("$resolver" android-code)"
windows_package="$("$resolver" windows-package)"
"$resolver" verify >/dev/null
[[ $source_tag == "v$version" ]] || {
	printf 'Release tag %s does not match canonical version v%s\n' \
		"$source_tag" "$version" >&2
	exit 1
}

deb="$(find_single "$input_dir/deb" '*.deb' 'Debian package')"
rpm="$(find_single "$input_dir/rpm" '*.rpm' 'RPM package')"
dmg="$(find_single "$input_dir/macos" '*.dmg' 'macOS DMG')"
appcast="$(find_single "$input_dir/macos" 'appcast.xml' 'Sparkle appcast')"
apple_metadata="$(find_single "$input_dir/macos" '*.build-info.json' 'direct macOS build metadata')"
apple_core="$(find_single "$input_dir/macos" 'VnidropCore-*.zip' 'Apple prebuilt core bundle')"
play_apk="$(find_single "$input_dir/play" '*-play-universal.apk' 'Play-signed APK')"
play_metadata="$(find_single "$input_dir/play" 'play-release.json' 'Play release metadata')"
msix="$(find_single "$input_dir/windows" '*.msix' 'Windows MSIX')"
msixupload="$(find_single "$input_dir/windows" '*.msixupload' 'Windows MSIX upload')"
windows_installer="$(find_single "$input_dir/windows" '*.exe' 'Windows direct installer')"
windows_metadata="$(find_single "$input_dir/windows" '*.build-info.json' 'Windows build metadata')"

[[ $(basename "$deb") == "vnidrop_${version}-1_amd64.deb" ]]
[[ $(basename "$rpm") == "vnidrop-${version}-1.x86_64.rpm" ]]
[[ $(basename "$dmg") == "VniDrop-${version}.dmg" ]]
[[ $(basename "$apple_core") == "VnidropCore-${version}.zip" ]]
[[ $(basename "$play_apk") == "VniDrop-${version}-${android_code}-play-universal.apk" ]]
[[ $(basename "$msix") == "VniDrop_${version}_x64.msix" ]]
[[ $(basename "$msixupload") == "VniDrop_${version}_x64.msixupload" ]]
[[ $(basename "$windows_installer") == "VniDrop_${version}_x64.exe" ]]
[[ $(jq -r '.productVersion' "$apple_metadata") == "$version" ]]
[[ $(jq -r '.distribution' "$apple_metadata") == direct ]]
[[ $(jq -r '.artifact' "$apple_metadata") == "$(basename "$dmg")" ]]
apple_direct_build="$(jq -r '.directBuildNumber' "$apple_metadata")"
[[ $apple_direct_build =~ ^[1-9][0-9]*(\.[0-9]+){0,2}$ ]] || {
	printf 'Invalid direct Apple build number: %s\n' "$apple_direct_build" >&2
	exit 1
}

deb_checksum="$(find_single "$input_dir/deb" '*.sha256' 'Debian checksum')"
rpm_checksum="$(find_single "$input_dir/rpm" '*.sha256' 'RPM checksum')"
windows_checksums="$(find_single "$input_dir/windows" 'SHA256SUMS' 'Windows checksums')"
play_checksums="$(find_single "$input_dir/play" 'SHA256SUMS' 'Play APK checksums')"
apple_core_checksum="$(find_single "$input_dir/macos" 'VnidropCore-*.zip.sha256' 'Apple prebuilt core checksum')"
verify_checksum_file "$deb_checksum"
verify_checksum_file "$rpm_checksum"
verify_checksum_file "$windows_checksums"
verify_checksum_file "$play_checksums"
verify_checksum_file "$apple_core_checksum"

[[ $(jq -r '.releaseStatus' "$play_metadata") == draft ]]
[[ $(jq -r '.releaseName' "$play_metadata") == "$version" ]]
[[ $(jq -r '.versionCode' "$play_metadata") == "$android_code" ]]
play_track="$(jq -r '.track' "$play_metadata")"
normalized_play_track="$(printf '%s' "$play_track" | tr '[:upper:]' '[:lower:]')"
[[ $normalized_play_track != production && $normalized_play_track != *:production ]]
[[ $(jq -r '.appVersion' "$windows_metadata") == "$version" ]]
[[ $(jq -r '.packageVersion' "$windows_metadata") == "$windows_package" ]]
[[ $(jq -r '.directInstaller.artifact' "$windows_metadata") == "$(basename "$windows_installer")" ]]
[[ $(jq -r '.directInstaller.unsigned' "$windows_metadata") == true ]]
[[ $(jq -r '.directInstaller.smartScreenWarningExpected' "$windows_metadata") == true ]]
grep -F "VniDrop-${version}.dmg" "$appcast" >/dev/null

mkdir -p "$output_dir"
[[ -z $(find "$output_dir" -mindepth 1 -maxdepth 1 -print -quit) ]] || {
	printf 'Release output directory must be empty: %s\n' "$output_dir" >&2
	exit 1
}
cp "$deb" "$rpm" "$dmg" "$appcast" "$play_apk" "$apple_core" "$windows_installer" "$output_dir/"

payloads=(
	"$output_dir/$(basename "$deb")"
	"$output_dir/$(basename "$rpm")"
	"$output_dir/$(basename "$dmg")"
	"$output_dir/$(basename "$appcast")"
	"$output_dir/$(basename "$play_apk")"
	"$output_dir/$(basename "$apple_core")"
	"$output_dir/$(basename "$windows_installer")"
)
files_json="$(
	for file in "${payloads[@]}"; do
		jq -n \
			--arg name "$(basename "$file")" \
			--arg sha256 "$(sha256sum "$file" | awk '{print $1}')" \
			--argjson bytes "$(file_size "$file")" \
			'{name: $name, sha256: $sha256, bytes: $bytes}'
	done | jq -s .
)"

jq -n \
	--arg productVersion "$version" \
	--arg releaseChannel "$("$resolver" channel)" \
	--arg tag "$source_tag" \
	--arg commit "$source_commit" \
	--arg androidVersionCode "$android_code" \
	--arg appleDirectBuildNumber "$apple_direct_build" \
	--arg windowsPackageVersion "$windows_package" \
	--arg windowsMsixUpload "$(basename "$msixupload")" \
	--arg windowsMsixUploadSha256 "$(sha256sum "$msixupload" | awk '{print $1}')" \
	--arg windowsDirectInstaller "$(basename "$windows_installer")" \
	--arg windowsDirectInstallerSha256 "$(sha256sum "$windows_installer" | awk '{print $1}')" \
	--arg playTrack "$play_track" \
	--arg playBundleSha256 "$(jq -r '.bundleSha256' "$play_metadata")" \
	--arg playCertificateSha256 "$(jq -r '.appSigningCertificateSha256' "$play_metadata")" \
	--argjson files "$files_json" \
	'{
		productVersion: $productVersion,
		releaseChannel: $releaseChannel,
		tag: $tag,
		sourceCommit: $commit,
		platformVersions: {
			androidVersionCode: ($androidVersionCode | tonumber),
			appleDirectBuildNumber: $appleDirectBuildNumber,
			windowsPackageVersion: $windowsPackageVersion
		},
		play: {
			track: $playTrack,
			status: "draft",
			bundleSha256: $playBundleSha256,
			appSigningCertificateSha256: $playCertificateSha256
		},
		windowsStore: {
			publicReleaseAsset: false,
			msixUpload: $windowsMsixUpload,
			sha256: $windowsMsixUploadSha256
		},
		windowsDirect: {
			publicReleaseAsset: true,
			installer: $windowsDirectInstaller,
			sha256: $windowsDirectInstallerSha256,
			unsigned: true,
			smartScreenWarningExpected: true
		},
		files: $files
	}' > "$output_dir/release-manifest.json"

(
	cd "$output_dir"
	sha256sum \
		"$(basename "$deb")" \
		"$(basename "$rpm")" \
		"$(basename "$dmg")" \
		"$(basename "$appcast")" \
		"$(basename "$play_apk")" \
		"$(basename "$apple_core")" \
		"$(basename "$windows_installer")" \
		release-manifest.json \
		> SHA256SUMS
)

printf 'Assembled public release assets in %s\n' "$output_dir"
printf 'Windows Store submission retained as workflow artifact: %s\n' \
	"$(basename "$msixupload")"
printf 'Unsigned Windows direct installer included as a public asset: %s\n' \
	"$(basename "$windows_installer")"
