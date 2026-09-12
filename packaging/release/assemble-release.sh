#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
resolver="$repo_root/packaging/version/resolve-version.sh"
"$resolver" verify >/dev/null
version="$("$resolver" product)"
options=()
if [[ -n ${VNIDROP_PREVIOUS_MANIFEST:-} ]]; then
  options+=(--previous "$VNIDROP_PREVIOUS_MANIFEST")
fi
python3 "$script_dir/assemble.py" \
  --input-dir "${VNIDROP_RELEASE_INPUT_DIR:-$repo_root/build/release/downloads}" \
  --output-dir "${VNIDROP_RELEASE_OUTPUT_DIR:-$repo_root/build/release/final}" \
  --version "$version" --tag "${VNIDROP_RELEASE_TAG:-${GITHUB_REF_NAME:-v$version}}" --commit "${VNIDROP_RELEASE_COMMIT:-${GITHUB_SHA:-local}}" \
  --android-code "$("$resolver" android-code)" --windows-package "$("$resolver" windows-package)" \
  --channel "$("$resolver" channel)" \
  --platforms "${VNIDROP_RELEASE_PLATFORMS-windows,linux,android,macos}" \
  --stores "${VNIDROP_RELEASE_STORES-windows,android,macos,ios}" "${options[@]}"
