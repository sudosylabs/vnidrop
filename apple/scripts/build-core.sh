#!/usr/bin/env bash
#
# Builds the VniDrop Rust core for Apple platforms and produces:
#   - apple/VnidropCore/vnidrop.xcframework   (static libs for device/sim/macOS)
#   - apple/VnidropCore/Sources/VnidropCore/Vnidrop.swift  (generated bindings)
#
# The Rust crate (crates/vnidrop) is NOT modified. Bindings are generated in
# UniFFI library mode from the compiled staticlib, so the Swift surface always
# matches the scaffolding baked into the library.
#
# Usage: apple/scripts/build-core.sh [debug|release]   (default: release)
set -euo pipefail

# Default to debug: the workspace `[profile.release]` uses thin LTO, which the
# current macOS toolchain miscompiles into corrupt host proc-macro dylibs
# ("mis-aligned LINKEDIT string pool"), breaking any release cross-compile. Debug
# static libs are correct and adequate for development and the simulator. For a
# release build, pass `release` AND disable LTO for proc-macros/build scripts via
# CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_LTO=false in a Cargo.toml profile — see the
# package README. The Rust crate itself is never modified.
PROFILE="${1:-debug}"
case "$PROFILE" in
	debug) CARGO_PROFILE_FLAG="" ;;
	release) CARGO_PROFILE_FLAG="--release" ;;
	*) echo "profile must be debug or release" >&2; exit 1 ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
APPLE_DIR="$REPO_ROOT/apple"
PKG_DIR="$APPLE_DIR/VnidropCore"
GEN_DIR="$PKG_DIR/Sources/VnidropCore"
TARGET_DIR="$REPO_ROOT/target"
BUILD_DIR="$APPLE_DIR/.build-core"

# Match the SwiftUI app's deployment targets (apple/project.yml) so the static
# libs are never built for a newer OS than the app links against.
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-18.2}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-15.0}"

# The workspace `[profile.dev] strip = "debuginfo"` corrupts host proc-macro
# dylibs on the current Apple toolchain ("mis-aligned LINKEDIT string pool"),
# which breaks compilation. The Gobley Xcode run-script uses the same override.
# This never touches the Rust crate — it only changes how the build is invoked.
export CARGO_PROFILE_DEV_STRIP=none

# The workspace `[profile.release] lto = "thin"` corrupts host proc-macro / build
# script dylibs when cross-compiling ("mis-aligned LINKEDIT string pool"). Cargo
# forbids overriding `lto` per build-override, so disable thin LTO for the whole
# release build here — the crate is still fully optimized (opt-level 3, debuginfo
# stripped), which is what shrinks the static lib. This never edits the Cargo crate.
export CARGO_PROFILE_RELEASE_LTO=false

IOS_TARGET="aarch64-apple-ios"
SIM_ARM_TARGET="aarch64-apple-ios-sim"
SIM_X64_TARGET="x86_64-apple-ios"
MAC_TARGET="aarch64-apple-darwin"

echo "==> Building vnidrop staticlib ($PROFILE) for Apple targets"
for t in "$IOS_TARGET" "$SIM_ARM_TARGET" "$SIM_X64_TARGET" "$MAC_TARGET"; do
	echo "    - $t"
	rustup target add "$t" >/dev/null 2>&1 || true
	( cd "$REPO_ROOT" && cargo build -p vnidrop --target "$t" $CARGO_PROFILE_FLAG )
done

LIB_SUBDIR="$PROFILE"
[ "$PROFILE" = "debug" ] && LIB_SUBDIR="debug"

MAC_LIB="$TARGET_DIR/$MAC_TARGET/$LIB_SUBDIR/libvnidrop.a"
IOS_LIB="$TARGET_DIR/$IOS_TARGET/$LIB_SUBDIR/libvnidrop.a"

# Fresh scratch dir for the bindgen output and the universal simulator lib.
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

# Combine the two simulator architectures into one universal static library so
# the xcframework works on both Apple Silicon and Intel simulators.
SIM_LIB="$BUILD_DIR/libvnidrop-sim.a"
lipo -create \
	"$TARGET_DIR/$SIM_ARM_TARGET/$LIB_SUBDIR/libvnidrop.a" \
	"$TARGET_DIR/$SIM_X64_TARGET/$LIB_SUBDIR/libvnidrop.a" \
	-output "$SIM_LIB"

echo "==> Generating Swift bindings (library mode)"
( cd "$REPO_ROOT" && cargo run -p uniffi-bindgen -- generate \
	--library "$MAC_LIB" \
	--language swift \
	--out-dir "$BUILD_DIR" )

# UniFFI emits: Vnidrop.swift, vnidropFFI.h, vnidropFFI.modulemap
mkdir -p "$GEN_DIR"
cp "$BUILD_DIR/Vnidrop.swift" "$GEN_DIR/Vnidrop.swift"

# Assemble a headers dir the xcframework can carry as the FFI module.
HEADERS_DIR="$BUILD_DIR/headers"
mkdir -p "$HEADERS_DIR"
cp "$BUILD_DIR/vnidropFFI.h" "$HEADERS_DIR/"
# The xcframework module map must be named module.modulemap.
cp "$BUILD_DIR/vnidropFFI.modulemap" "$HEADERS_DIR/module.modulemap"

echo "==> Assembling xcframework"
XCFRAMEWORK="$PKG_DIR/vnidrop.xcframework"
rm -rf "$XCFRAMEWORK"
xcodebuild -create-xcframework \
	-library "$IOS_LIB" -headers "$HEADERS_DIR" \
	-library "$SIM_LIB" -headers "$HEADERS_DIR" \
	-library "$MAC_LIB" -headers "$HEADERS_DIR" \
	-output "$XCFRAMEWORK"

echo "==> Done."
echo "    xcframework: $XCFRAMEWORK"
echo "    bindings:    $GEN_DIR/Vnidrop.swift"
