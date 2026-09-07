#!/usr/bin/env bash
set -euo pipefail
data_dir=${1:?Usage: verify-mime.sh staged-share-directory}
script_dir=$(cd "$(dirname "$0")" && pwd)
isolated_home=$(mktemp -d)
trap 'rm -rf "$isolated_home"' EXIT

# Ignore host registrations and desktop-dependent xdg-mime backends in CI.
export XDG_DATA_HOME="$isolated_home"
export XDG_DATA_DIRS="$data_dir"
export LC_ALL=C
update-mime-database "$data_dir/mime"
actual=$(gio info -a standard::content-type "$script_dir/fixture.vnd" | sed -n 's/^ *standard::content-type: //p')
if [[ $actual != application/vnd.vnidrop.transfer ]]; then
  echo "Packaged invitation MIME type: expected application/vnd.vnidrop.transfer, got $actual" >&2
  exit 1
fi
