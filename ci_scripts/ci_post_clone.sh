#!/bin/bash
#
# Xcode Cloud post-clone step.
#
# The Apple Xcode project is generated (XcodeGen) and gitignored, and it links a
# prebuilt Rust XCFramework plus generated localization/config files. Xcode Cloud
# only checks out the repository, so this script:
#   1. installs the non-Rust build tooling (swiftlint, xcodegen, bun);
#   2. downloads the prebuilt core (vnidrop.xcframework + Vnidrop.swift) from the
#      matching GitHub Release asset — we never build Rust here;
#   3. reproduces `make apple-project` minus the Rust `apple-core` step.
#
# Xcode Cloud runs this from the `ci_scripts` directory; CI_PRIMARY_REPOSITORY_PATH
# points at the checked-out repository root.
set -euo pipefail

REPO_ROOT="${CI_PRIMARY_REPOSITORY_PATH:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$REPO_ROOT"

echo "==> Installing build tooling (Homebrew)"
# swiftlint: enforced by a build phase (fails the build if missing).
# xcodegen: generates apple/VniDrop.xcodeproj from apple/project.yml.
brew install swiftlint xcodegen

echo "==> Installing Bun (localization generator)"
if ! command -v bun >/dev/null 2>&1; then
	curl -fsSL https://bun.sh/install | bash
fi
export BUN_INSTALL="${BUN_INSTALL:-$HOME/.bun}"
export PATH="$BUN_INSTALL/bin:$PATH"

# --- Prebuilt core: download instead of building Rust -------------------------
# The Apple core (xcframework + UniFFI bindings) is published as a release asset
# by apple/scripts/package-core.sh. See docs at the top of that script.
VERSION="$(packaging/version/resolve-version.sh product)"
CORE_REPO="${VNIDROP_CORE_REPO:-sudosylabs/vnidrop}"
CORE_TAG="${VNIDROP_CORE_TAG:-v$VERSION}"
CORE_ZIP="VnidropCore-$VERSION.zip"
CORE_BASE_URL="https://github.com/$CORE_REPO/releases/download/$CORE_TAG"

PKG_DIR="$REPO_ROOT/apple/VnidropCore"
DOWNLOAD_DIR="$(mktemp -d)"
trap 'rm -rf "$DOWNLOAD_DIR"' EXIT

echo "==> Downloading prebuilt core $CORE_ZIP from $CORE_REPO@$CORE_TAG"
curl -fsSL "$CORE_BASE_URL/$CORE_ZIP" -o "$DOWNLOAD_DIR/$CORE_ZIP"
# The release publishes one aggregate SHA256SUMS covering every public asset —
# packaging/release/assemble-release.sh does not upload the per-file .sha256 that
# package-core.sh writes locally. Pull the core's line out of it.
curl -fsSL "$CORE_BASE_URL/SHA256SUMS" -o "$DOWNLOAD_DIR/SHA256SUMS"

echo "==> Verifying checksum"
(
	cd "$DOWNLOAD_DIR"
	grep -F "  $CORE_ZIP" SHA256SUMS > "$CORE_ZIP.sha256"
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum --check "$CORE_ZIP.sha256"
	else
		shasum -a 256 --check "$CORE_ZIP.sha256"
	fi
)

echo "==> Installing core into apple/VnidropCore"
unzip -q -o "$DOWNLOAD_DIR/$CORE_ZIP" -d "$DOWNLOAD_DIR/extracted"
# Zip root holds: vnidrop.xcframework/ and Vnidrop.swift (see package-core.sh).
rm -rf "$PKG_DIR/vnidrop.xcframework"
cp -R "$DOWNLOAD_DIR/extracted/vnidrop.xcframework" "$PKG_DIR/vnidrop.xcframework"
mkdir -p "$PKG_DIR/Sources/VnidropCore"
cp "$DOWNLOAD_DIR/extracted/Vnidrop.swift" "$PKG_DIR/Sources/VnidropCore/Vnidrop.swift"

# --- Generate the project (everything except the Rust core) -------------------
echo "==> Generating localization, version/app config, and the Xcode project"
make localization apple-version-config apple-app-config
(cd "$REPO_ROOT/apple" && xcodegen generate)

echo "==> ci_post_clone complete"
