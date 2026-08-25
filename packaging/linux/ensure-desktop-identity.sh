#!/usr/bin/env bash

set -euo pipefail

expected_entry='StartupWMClass=com-vnidrop-app-MainKt'
mode=write

if [[ ${1:-} == --check ]]; then
	mode=check
	shift
fi

if (( $# != 1 )); then
	echo "Usage: $0 [--check] <desktop-entry>" >&2
	exit 2
fi

desktop_entry=$1
[[ -f $desktop_entry ]] || {
	echo "Desktop entry not found: $desktop_entry" >&2
	exit 1
}

if [[ $mode == check ]]; then
	entry_count=$(awk '/^StartupWMClass=/{ count++ } END { print count + 0 }' "$desktop_entry")
	if (( entry_count != 1 )) || ! grep -Fxq "$expected_entry" "$desktop_entry"; then
		echo "Desktop entry must contain exactly '$expected_entry': $desktop_entry" >&2
		exit 1
	fi
	exit 0
fi

updated_entry=$(mktemp "${TMPDIR:-/tmp}/vnidrop-desktop-entry.XXXXXX")
trap 'rm -f "$updated_entry"' EXIT

awk -v expected="$expected_entry" '
	/^StartupWMClass=/ {
		if (!found) {
			print expected
			found = 1
		}
		next
	}
	{ print }
	END {
		if (!found) {
			print expected
		}
	}
' "$desktop_entry" > "$updated_entry"

cp "$updated_entry" "$desktop_entry"
