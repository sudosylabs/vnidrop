#!/usr/bin/env bash
#
# Packages the prebuilt Apple core into a single zip + checksum, for attaching to
# the GitHub Release. Lets a consumer (e.g. Xcode Cloud) use the compiled core
# instead of installing Rust and running build-core.sh. Run AFTER the core exists
# (apple/scripts/build-core.sh, or `make apple-core` / `make build-apple-dmg`).
#
# The bundle carries both build outputs of build-core.sh:
#   - vnidrop.xcframework   (static libs + the FFI module. Release builds carry
#                            device and macOS only — no simulator slice, which
#                            nothing shipped can use. VNIDROP_APPLE_SIMULATOR=1
#                            adds it back.)
#   - Vnidrop.swift         (generated UniFFI bindings — a plain source file, not
#                            part of the xcframework, so it must ship alongside)
#
# Produces (under apple/dist):
#   VnidropCore-<version>.zip
#   VnidropCore-<version>.zip.sha256   (sha256sum(1)/shasum-compatible format)
#
# Zip layout (root):
#   vnidrop.xcframework/
#   Vnidrop.swift
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
APPLE_DIR="$REPO_ROOT/apple"
PKG_DIR="$APPLE_DIR/VnidropCore"
XCFRAMEWORK="$PKG_DIR/vnidrop.xcframework"
BINDINGS="$PKG_DIR/Sources/VnidropCore/Vnidrop.swift"
DIST_DIR="$APPLE_DIR/dist"

VERSION="$("$REPO_ROOT/packaging/version/resolve-version.sh" product)"
NAME="VnidropCore-$VERSION"
ZIP="$DIST_DIR/$NAME.zip"
CHECKSUM="$ZIP.sha256"

[ -d "$XCFRAMEWORK" ] || {
	echo "error: missing xcframework: $XCFRAMEWORK" >&2
	echo "       build the core first (apple/scripts/build-core.sh)." >&2
	exit 1
}
[ -f "$BINDINGS" ] || {
	echo "error: missing generated bindings: $BINDINGS" >&2
	echo "       build the core first (apple/scripts/build-core.sh)." >&2
	exit 1
}

mkdir -p "$DIST_DIR"
rm -f "$ZIP" "$CHECKSUM"

# Stage a clean tree so the zip root holds exactly the two payloads (no absolute
# paths or stray parent directories leak into the archive).
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$XCFRAMEWORK" "$STAGE/vnidrop.xcframework"
cp "$BINDINGS" "$STAGE/Vnidrop.swift"

# -X drops extra file attributes for a stabler archive across machines.
( cd "$STAGE" && zip -q -r -X "$ZIP" vnidrop.xcframework Vnidrop.swift )

# sha256sum on Linux; shasum -a 256 on macOS. Both emit "<hash>  <name>", which
# `sha256sum --check` (used by assemble-release.sh) accepts.
(
	cd "$DIST_DIR"
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$NAME.zip" > "$NAME.zip.sha256"
	else
		shasum -a 256 "$NAME.zip" > "$NAME.zip.sha256"
	fi
)

echo "==> Packaged prebuilt core"
echo "    zip:      $ZIP"
echo "    checksum: $CHECKSUM"
