#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

dry_run="$(make -n -C "$repo_root" build-apple-dmg)"
localization_line="$(
	printf '%s\n' "$dry_run" |
		awk '/bun run generate/ {print NR; exit}'
)"
build_line="$(
	printf '%s\n' "$dry_run" |
		awk '/apple\/scripts\/build-dmg\.sh/ {print NR; exit}'
)"
[[ -n $localization_line && -n $build_line && $localization_line -lt $build_line ]] || {
	printf 'build-apple-dmg must generate localization before building the DMG\n' >&2
	exit 1
}

grep -F 'run: make build-apple-dmg' \
	"$repo_root/.github/workflows/apple-release.yml" >/dev/null || {
	printf 'Apple release workflow must use the generated-input-aware Make target\n' >&2
	exit 1
}

store_reconfigure_line="$(
	awk '/msstore reconfigure/ {print NR; exit}' \
		"$repo_root/.github/workflows/release.yml"
)"
store_settings_line="$(
	awk '/msstore settings --enableTelemetry false/ {print NR; exit}' \
		"$repo_root/.github/workflows/release.yml"
)"
[[ -n $store_reconfigure_line &&
	-n $store_settings_line &&
	$store_reconfigure_line -lt $store_settings_line ]] || {
	printf 'Microsoft Store CLI credentials must be configured before changing settings\n' >&2
	exit 1
}

signing_line="$(
	awk '/sign-exported-app\.sh/ {print NR; exit}' \
		"$repo_root/apple/scripts/build-dmg.sh"
)"
dmg_line="$(
	awk '/echo "==> Building DMG"/ {print NR; exit}' \
		"$repo_root/apple/scripts/build-dmg.sh"
)"
[[ -n $signing_line && -n $dmg_line && $signing_line -lt $dmg_line ]] || {
	printf 'The exported app must enforce hardened-runtime signing before DMG creation\n' >&2
	exit 1
}

printf 'Release configuration tests passed.\n'
