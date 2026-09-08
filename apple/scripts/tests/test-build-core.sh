#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/vnidrop-apple-core.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/apple/scripts" "$scratch/bin"
cp "$script_dir/../build-core.sh" "$scratch/apple/scripts/"
printf '#!/usr/bin/env bash\nexit 0\n' > "$scratch/bin/rustup"
cat > "$scratch/bin/cargo" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$FAKE_CARGO_CALLS"
if [[ $1 == run ]]; then
  while [[ $# -gt 0 ]]; do
    if [[ $1 == --out-dir ]]; then
      mkdir -p "$2"
      for name in Vnidrop.swift vnidropFFI.h vnidropFFI.modulemap; do echo fixture > "$2/$name"; done
      break
    fi
    shift
  done
fi
SCRIPT
cat > "$scratch/bin/xcodebuild" <<'SCRIPT'
#!/usr/bin/env bash
printf '%s\n' "$*" > "$FAKE_XCODE_CALLS"
SCRIPT
chmod +x "$scratch/bin/"*
export PATH="$scratch/bin:$PATH"
export FAKE_CARGO_CALLS="$scratch/cargo-calls"
export FAKE_XCODE_CALLS="$scratch/xcode-calls"
export VNIDROP_APPLE_SIMULATOR=0

for ios in 0 1; do
  : > "$FAKE_CARGO_CALLS"
  VNIDROP_APPLE_IOS="$ios" bash "$scratch/apple/scripts/build-core.sh" release >/dev/null
  grep -F -- '--target aarch64-apple-darwin' "$FAKE_CARGO_CALLS" >/dev/null
  grep -F -- '--release' "$FAKE_CARGO_CALLS" >/dev/null
  grep -F '/aarch64-apple-darwin/release/libvnidrop.a' "$FAKE_XCODE_CALLS" >/dev/null
  if [[ $ios == 1 ]]; then
    grep -F -- '--target aarch64-apple-ios' "$FAKE_CARGO_CALLS" >/dev/null
    grep -F '/aarch64-apple-ios/release/libvnidrop.a' "$FAKE_XCODE_CALLS" >/dev/null
  else
    if grep -F 'aarch64-apple-ios' "$FAKE_CARGO_CALLS" "$FAKE_XCODE_CALLS" >/dev/null; then
      echo 'macOS-only core must not build or package iOS slices' >&2
      exit 1
    fi
  fi
done
if VNIDROP_APPLE_IOS=0 VNIDROP_APPLE_SIMULATOR=1 bash "$scratch/apple/scripts/build-core.sh" release >/dev/null 2>&1; then
  echo 'Simulator requests without iOS must fail' >&2
  exit 1
fi
printf 'Apple core platform tests passed.\n'
