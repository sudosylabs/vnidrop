#!/usr/bin/env bash
set -euo pipefail
packaging_dir=$(cd "$(dirname "$0")/.." && pwd)
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/share/mime/packages" "$fixture/bin"
cp "$packaging_dir/com.vnidrop.VniDrop.xml" "$fixture/share/mime/packages/"
printf '#!/bin/sh\nexit 127\n' > "$fixture/bin/mimetype"
chmod +x "$fixture/bin/mimetype"
export PATH="$fixture/bin:$PATH"
export XDG_CURRENT_DESKTOP=X-Generic
unset DISPLAY WAYLAND_DISPLAY

bash "$packaging_dir/verify-mime.sh" "$fixture/share"

# A host registration must not hide a missing registration in the package.
cp -r "$fixture/share" "$fixture/host-share"
export XDG_DATA_HOME="$fixture/host-share"
rm "$fixture/share/mime/packages/com.vnidrop.VniDrop.xml"
if bash "$packaging_dir/verify-mime.sh" "$fixture/share" > "$fixture/error.log" 2>&1; then
  echo 'Expected missing packaged MIME registration to fail verification.' >&2
  exit 1
fi
grep -q 'Packaged invitation MIME type: expected' "$fixture/error.log"
echo 'Headless package MIME verification tests passed.'
