#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 3 ]]; then
	printf 'Usage: %s <app-bundle> <signing-identity> <entitlements>\n' "$0" >&2
	exit 2
fi

app=$1
signing_identity=$2
entitlements=$3

[[ -d $app ]] || {
	printf 'error: exported app bundle does not exist: %s\n' "$app" >&2
	exit 1
}
[[ -n $signing_identity ]] || {
	printf 'error: signing identity is empty\n' >&2
	exit 1
}
[[ -f $entitlements ]] || {
	printf 'error: entitlements file does not exist: %s\n' "$entitlements" >&2
	exit 1
}

codesign \
	--force \
	--sign "$signing_identity" \
	--options runtime \
	--timestamp \
	--entitlements "$entitlements" \
	"$app"
codesign --verify --deep --strict --verbose=2 "$app"

signature_details="$(codesign --display --verbose=4 "$app" 2>&1)"
printf '%s\n' "$signature_details"
printf '%s\n' "$signature_details" |
	grep -Eq 'flags=.*\(runtime([^)]*)?\)' || {
		printf 'error: exported app signature does not enable the hardened runtime\n' >&2
		exit 1
	}
