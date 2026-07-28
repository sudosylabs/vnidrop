#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/vnidrop-release-test.XXXXXX")"
trap 'rm -rf "$fixture_root"' EXIT

input_dir="$fixture_root/input"
output_dir="$fixture_root/output"
mkdir -p \
	"$input_dir/deb" \
	"$input_dir/rpm" \
	"$input_dir/macos" \
	"$input_dir/play" \
	"$input_dir/windows"

version="$("$repo_root/packaging/version/resolve-version.sh" product)"
android_code="$("$repo_root/packaging/version/resolve-version.sh" android-code)"
windows_package="$("$repo_root/packaging/version/resolve-version.sh" windows-package)"
apple_direct_build=20260728.1432.17

printf 'deb\n' > "$input_dir/deb/vnidrop_${version}-1_amd64.deb"
printf 'rpm\n' > "$input_dir/rpm/vnidrop-${version}-1.x86_64.rpm"
printf 'dmg\n' > "$input_dir/macos/VniDrop-${version}.dmg"
printf '<url>VniDrop-%s.dmg</url>\n' "$version" > "$input_dir/macos/appcast.xml"
printf 'apk\n' > "$input_dir/play/VniDrop-${version}-${android_code}-play-universal.apk"
printf 'msix\n' > "$input_dir/windows/VniDrop_${version}_x64.msix"
printf 'msixupload\n' > "$input_dir/windows/VniDrop_${version}_x64.msixupload"

jq -n \
	--arg productVersion "$version" \
	--arg directBuildNumber "$apple_direct_build" \
	--arg artifact "VniDrop-${version}.dmg" \
	'{
		productVersion: $productVersion,
		directBuildNumber: $directBuildNumber,
		distribution: "direct",
		artifact: $artifact
	}' > "$input_dir/macos/VniDrop-${version}.build-info.json"

jq -n \
	--arg releaseName "$version" \
	--argjson versionCode "$android_code" \
	'{
		releaseStatus: "draft",
		releaseName: $releaseName,
		versionCode: $versionCode,
		track: "closed-beta",
		bundleSha256: "bundle-sha",
		appSigningCertificateSha256: "certificate-sha"
	}' > "$input_dir/play/play-release.json"

jq -n \
	--arg appVersion "$version" \
	--arg packageVersion "$windows_package" \
	'{appVersion: $appVersion, packageVersion: $packageVersion}' \
	> "$input_dir/windows/VniDrop_${version}_x64.build-info.json"

(
	cd "$input_dir/deb"
	sha256sum "vnidrop_${version}-1_amd64.deb" \
		> "vnidrop_${version}-1_amd64.deb.sha256"
)
(
	cd "$input_dir/rpm"
	sha256sum "vnidrop-${version}-1.x86_64.rpm" \
		> "vnidrop-${version}-1.x86_64.rpm.sha256"
)
(
	cd "$input_dir/play"
	sha256sum \
		"VniDrop-${version}-${android_code}-play-universal.apk" \
		play-release.json \
		> SHA256SUMS
)
(
	cd "$input_dir/windows"
	sha256sum \
		"VniDrop_${version}_x64.msix" \
		"VniDrop_${version}_x64.msixupload" \
		"VniDrop_${version}_x64.build-info.json" \
		> SHA256SUMS
)

GITHUB_REF_NAME="v$version" \
	GITHUB_SHA=fixture-commit \
	VNIDROP_RELEASE_INPUT_DIR="$input_dir" \
	VNIDROP_RELEASE_OUTPUT_DIR="$output_dir" \
	"$script_dir/assemble-release.sh" >/dev/null

expected_public_files=(
	"SHA256SUMS"
	"VniDrop-${version}-${android_code}-play-universal.apk"
	"VniDrop-${version}.dmg"
	"appcast.xml"
	"release-manifest.json"
	"vnidrop-${version}-1.x86_64.rpm"
	"vnidrop_${version}-1_amd64.deb"
)
actual_public_files=()
while IFS= read -r file; do
	actual_public_files+=("$(basename "$file")")
done < <(find "$output_dir" -maxdepth 1 -type f -print | sort)
[[ ${actual_public_files[*]} == "${expected_public_files[*]}" ]]

[[ $(jq -r '.productVersion' "$output_dir/release-manifest.json") == "$version" ]]
[[ $(jq -r '.platformVersions.appleDirectBuildNumber' \
	"$output_dir/release-manifest.json") == "$apple_direct_build" ]]
[[ $(jq -r '.play.status' "$output_dir/release-manifest.json") == draft ]]
[[ $(jq -r '.windowsStore.publicReleaseAsset' "$output_dir/release-manifest.json") == false ]]
(
	cd "$output_dir"
	sha256sum --check SHA256SUMS >/dev/null
)
