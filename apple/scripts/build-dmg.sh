#!/usr/bin/env bash
#
# Builds the direct-download macOS artifact: a Developer ID–signed, notarized
# .dmg of the VniDropDirect target (the Sparkle-enabled build). Produces:
#   - apple/dist/VniDrop-<version>.dmg   (signed + stapled when notarizing)
#
# This is the direct-distribution counterpart to the App Store archive flow; it
# never touches the App Store `VniDrop` target. The Rust crate is not modified.
#
# Usage: apple/scripts/build-dmg.sh
#
# Environment:
#   DEVELOPER_ID_APP   Codesign identity, e.g. "Developer ID Application: … (TEAMID)".
#                      Auto-detected from the keychain when unset.
#   DEVELOPMENT_TEAM   Apple team ID (10 chars). Auto-derived from the identity.
#   NOTARY_PROFILE     Name of a `xcrun notarytool store-credentials` keychain
#                      profile. When set, the DMG is notarized and stapled; when
#                      unset the build still produces a signed DMG and prints the
#                      pending notarization step (useful before creds exist).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
APPLE_DIR="$REPO_ROOT/apple"
DIST_DIR="$APPLE_DIR/dist"
BUILD_DIR="$APPLE_DIR/.build-dmg"
PROJECT="$APPLE_DIR/VniDrop.xcodeproj"
SCHEME="VniDropDirect"
CONFIG="Release-Direct"
APP_NAME="VniDrop"
VERSION_RESOLVER="$REPO_ROOT/packaging/version/resolve-version.sh"

VERSION="$("$VERSION_RESOLVER" product)"
BUILD_NUMBER="$("$VERSION_RESOLVER" apple-build)"
"$VERSION_RESOLVER" verify >/dev/null

# --- Resolve signing identity ------------------------------------------------
if [ -z "${DEVELOPER_ID_APP:-}" ]; then
	DEVELOPER_ID_APP="$(security find-identity -v -p codesigning 2>/dev/null \
		| sed -nE 's/.*"(Developer ID Application: [^"]+)".*/\1/p' | head -1)"
fi
if [ -z "${DEVELOPER_ID_APP:-}" ]; then
	echo "error: no 'Developer ID Application' identity found in the keychain." >&2
	echo "       Create one in Xcode ▸ Settings ▸ Accounts, or set DEVELOPER_ID_APP." >&2
	exit 1
fi
if [ -z "${DEVELOPMENT_TEAM:-}" ]; then
	# The team ID is the 10-char code in the trailing parenthesis of the identity.
	DEVELOPMENT_TEAM="$(printf '%s' "$DEVELOPER_ID_APP" | sed -nE 's/.*\(([A-Z0-9]{10})\)$/\1/p')"
fi
echo "==> Direct build v$VERSION (CFBundleVersion $BUILD_NUMBER)"
echo "    identity: $DEVELOPER_ID_APP"
echo "    team:     ${DEVELOPMENT_TEAM:-<unknown>}"

# --- Build core + regenerate project ----------------------------------------
# Release core needs LTO disabled (workspace thin-LTO miscompiles proc-macros).
echo "==> Building Rust core (release)"
CARGO_PROFILE_RELEASE_LTO=false "$SCRIPT_DIR/build-core.sh" release
echo "==> Regenerating Xcode project"
( cd "$APPLE_DIR" && xcodegen generate >/dev/null )

rm -rf "$BUILD_DIR" && mkdir -p "$BUILD_DIR" "$DIST_DIR"
ARCHIVE="$BUILD_DIR/$APP_NAME.xcarchive"
EXPORT_DIR="$BUILD_DIR/export"

# --- Archive + export (Developer ID) ----------------------------------------
echo "==> Archiving $SCHEME ($CONFIG)"
xcodebuild archive \
	-project "$PROJECT" \
	-scheme "$SCHEME" \
	-configuration "$CONFIG" \
	-destination 'generic/platform=macOS' \
	-archivePath "$ARCHIVE" \
	MARKETING_VERSION="$VERSION" \
	DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM" \
	CODE_SIGN_STYLE=Manual \
	CODE_SIGN_IDENTITY="$DEVELOPER_ID_APP" \
	| xcbeautify 2>/dev/null || true
[ -d "$ARCHIVE" ] || { echo "error: archive failed" >&2; exit 1; }

echo "==> Exporting Developer ID app"
EXPORT_OPTS="$BUILD_DIR/ExportOptions.plist"
sed "s/\${DEVELOPMENT_TEAM}/$DEVELOPMENT_TEAM/" \
	"$SCRIPT_DIR/ExportOptions-DeveloperID.plist" > "$EXPORT_OPTS"
xcodebuild -exportArchive \
	-archivePath "$ARCHIVE" \
	-exportPath "$EXPORT_DIR" \
	-exportOptionsPlist "$EXPORT_OPTS"
APP="$EXPORT_DIR/$APP_NAME.app"
[ -d "$APP" ] || { echo "error: export failed" >&2; exit 1; }

# --- Build the DMG -----------------------------------------------------------
DMG="$DIST_DIR/$APP_NAME-$VERSION.dmg"
rm -f "$DMG"
STAGING="$BUILD_DIR/dmg-staging"
rm -rf "$STAGING" && mkdir -p "$STAGING"
cp -R "$APP" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

echo "==> Building DMG"
if command -v create-dmg >/dev/null 2>&1; then
	create-dmg \
		--volname "$APP_NAME" \
		--app-drop-link 380 205 \
		--icon "$APP_NAME.app" 130 205 \
		--window-size 540 380 \
		--no-internet-enable \
		"$DMG" "$STAGING" >/dev/null || {
			# create-dmg exits non-zero if it can't set the fancy layout; fall back.
			[ -f "$DMG" ] || hdiutil create -volname "$APP_NAME" -srcfolder "$STAGING" \
				-ov -format UDZO "$DMG" >/dev/null
		}
else
	hdiutil create -volname "$APP_NAME" -srcfolder "$STAGING" \
		-ov -format UDZO "$DMG" >/dev/null
fi

echo "==> Signing DMG"
codesign --force --sign "$DEVELOPER_ID_APP" --timestamp "$DMG"

# --- Notarize + staple -------------------------------------------------------
if [ -n "${NOTARY_PROFILE:-}" ]; then
	echo "==> Notarizing (profile: $NOTARY_PROFILE)"
	xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
	echo "==> Stapling"
	xcrun stapler staple "$DMG"
	xcrun stapler validate "$DMG"
	spctl -a -vvv --type install "$DMG" || true
else
	echo "==> NOTARY_PROFILE unset — skipping notarization."
	echo "    The DMG is signed but NOT notarized; Gatekeeper will block it until"
	echo "    you run 'xcrun notarytool store-credentials' and re-run with NOTARY_PROFILE set."
fi

SIZE="$(stat -f%z "$DMG")"
echo "==> Done."
echo "    dmg:     $DMG"
echo "    version: $VERSION"
echo "    size:    $SIZE bytes"
