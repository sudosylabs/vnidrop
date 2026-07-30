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

printf 'Release configuration tests passed.\n'
