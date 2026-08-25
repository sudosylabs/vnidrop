#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../.." && pwd)"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/vnidrop-linux-desktop-test.XXXXXX")"
trap 'rm -rf "$fixture_root"' EXIT

desktop_entry="$fixture_root/vnidrop.desktop"
printf '%s\n' \
	'[Desktop Entry]' \
	'Type=Application' \
	'Name=VniDrop' \
	'Exec=/opt/vnidrop/bin/VniDrop' \
	> "$desktop_entry"

"$repo_root/packaging/linux/ensure-desktop-identity.sh" "$desktop_entry"

grep -Fxq 'StartupWMClass=com-vnidrop-app-MainKt' "$desktop_entry"
[[ $(grep -c '^StartupWMClass=' "$desktop_entry") -eq 1 ]]
"$repo_root/packaging/linux/ensure-desktop-identity.sh" --check "$desktop_entry"

sed -i.bak 's/^StartupWMClass=.*/StartupWMClass=wrong-window-class/' "$desktop_entry"
rm "$desktop_entry.bak"

if "$repo_root/packaging/linux/ensure-desktop-identity.sh" --check "$desktop_entry" 2>/dev/null; then
	echo 'Expected identity verification to reject the wrong window class' >&2
	exit 1
fi

"$repo_root/packaging/linux/ensure-desktop-identity.sh" "$desktop_entry"

grep -Fxq 'StartupWMClass=com-vnidrop-app-MainKt' "$desktop_entry"
[[ $(grep -c '^StartupWMClass=' "$desktop_entry") -eq 1 ]]
