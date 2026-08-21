#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
output_file="$(mktemp)"
trap 'rm -f "$output_file"' EXIT

if make --no-print-directory -C "$ROOT" open-apple APPLE_CODE_SIGNING=NO OPEN=true >"$output_file" 2>&1; then
  printf 'expected open-apple to reject an unsigned build\n' >&2
  exit 1
fi

grep -q 'requires a signed macOS app' "$output_file"
