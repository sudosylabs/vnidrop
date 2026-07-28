#!/usr/bin/env bash
#
# Generates/updates the Sparkle appcast for the direct-download build. Runs
# Sparkle's `generate_appcast` over the DMGs in apple/dist/, writing:
#   - apple/dist/appcast.xml
#
# The <enclosure> URLs point at the matching GitHub Release download assets, and
# each item is signed with the project's EdDSA key (from a key file or the
# keychain). The resulting appcast.xml is uploaded as a release asset; the app's
# SUFeedURL (/releases/latest/download/appcast.xml) always resolves to the newest.
#
# Usage: apple/scripts/generate-appcast.sh
#
# Environment:
#   DIST_DIR              Folder holding the DMG(s). Default: apple/dist
#   SPARKLE_BIN           Dir containing generate_appcast. Auto-located when unset.
#   SPARKLE_ED_KEY_FILE   Path to the EdDSA private key file. When unset,
#                         generate_appcast reads the key from the login keychain.
#   RELEASE_REPO          owner/repo for enclosure URLs. Default: sudosylabs/vnidrop
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
APPLE_DIR="$REPO_ROOT/apple"
DIST_DIR="${DIST_DIR:-$APPLE_DIR/dist}"
RELEASE_REPO="${RELEASE_REPO:-sudosylabs/vnidrop}"
VERSION_RESOLVER="$REPO_ROOT/packaging/version/resolve-version.sh"

VERSION="$("$VERSION_RESOLVER" product)"
"$VERSION_RESOLVER" verify >/dev/null

# Enclosure URLs resolve to the specific release's assets.
DOWNLOAD_PREFIX="https://github.com/$RELEASE_REPO/releases/download/v$VERSION"

# --- Locate generate_appcast -------------------------------------------------
find_tool() {
	local name="$1"
	if [ -n "${SPARKLE_BIN:-}" ] && [ -x "$SPARKLE_BIN/$name" ]; then
		printf '%s' "$SPARKLE_BIN/$name"; return 0
	fi
	if command -v "$name" >/dev/null 2>&1; then command -v "$name"; return 0; fi
	# Sparkle SPM artifact bundle lands under DerivedData SourcePackages.
	local dd="${APPLE_DERIVED_DATA:-$HOME/Library/Developer/Xcode/DerivedData}"
	local hit
	hit="$(find "$dd" "$HOME/Library/Caches/org.swift.swiftpm" -type f -name "$name" \
		-perm -111 2>/dev/null | head -1 || true)"
	[ -n "$hit" ] && { printf '%s' "$hit"; return 0; }
	return 1
}
GENERATE_APPCAST="$(find_tool generate_appcast || true)"
if [ -z "$GENERATE_APPCAST" ]; then
	echo "error: generate_appcast not found. Set SPARKLE_BIN to Sparkle's bin/ dir" >&2
	echo "       (download from https://github.com/sparkle-project/Sparkle/releases)." >&2
	exit 1
fi
echo "==> Using $GENERATE_APPCAST"

# --- Generate ----------------------------------------------------------------
args=( --download-url-prefix "$DOWNLOAD_PREFIX/" -o "$DIST_DIR/appcast.xml" )
if [ -n "${SPARKLE_ED_KEY_FILE:-}" ]; then
	args+=( --ed-key-file "$SPARKLE_ED_KEY_FILE" )
fi
echo "==> Generating appcast (v$VERSION) → $DIST_DIR/appcast.xml"
"$GENERATE_APPCAST" "${args[@]}" "$DIST_DIR"

echo "==> Done. Enclosure prefix: $DOWNLOAD_PREFIX/"
