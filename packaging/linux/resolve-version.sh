#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
resolver="$script_dir/../version/resolve-version.sh"
version="$("$resolver" product)"
"$resolver" verify >/dev/null

if [[ -n ${1:-} && $1 != "$version" ]]; then
	printf 'Version overrides are not supported; version.properties declares %s\n' "$version" >&2
	exit 1
fi

printf '%s\n' "$version"
