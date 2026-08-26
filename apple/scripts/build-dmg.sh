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
# Read the bundle id from project.yml rather than restating it, so the signed
# keychain access group can never drift from the app's actual identity.
APP_BUNDLE_ID="$(sed -nE 's/^[[:space:]]*PRODUCT_BUNDLE_IDENTIFIER:[[:space:]]*(.+)$/\1/p' \
	"$APPLE_DIR/project.yml" | head -1)"
[ -n "$APP_BUNDLE_ID" ] || {
	echo "error: could not read PRODUCT_BUNDLE_IDENTIFIER from project.yml" >&2
	exit 1
}
VERSION_RESOLVER="$REPO_ROOT/packaging/version/resolve-version.sh"
VERSION_CONFIG_GENERATOR="$REPO_ROOT/packaging/version/generate-apple-xcconfig.sh"
APP_CONFIG_GENERATOR="$SCRIPT_DIR/generate-appconfig.sh"

VERSION="$("$VERSION_RESOLVER" product)"
export VNIDROP_BUILD_TIME_UTC="${VNIDROP_BUILD_TIME_UTC:-$(date -u +%Y%m%d%H%M%S)}"
BUILD_NUMBER="$("$VERSION_RESOLVER" apple-direct-build)"
"$VERSION_RESOLVER" verify >/dev/null
BUILD_METADATA="$DIST_DIR/$APP_NAME-$VERSION.build-info.json"

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
"$VERSION_CONFIG_GENERATOR" all
# AppConfig.swift is gitignored codegen — a clean CI checkout has none.
"$APP_CONFIG_GENERATOR"
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
ACTUAL_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
	"$APP/Contents/Info.plist")"
ACTUAL_BUILD="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' \
	"$APP/Contents/Info.plist")"
[ "$ACTUAL_VERSION" = "$VERSION" ] || {
	echo "error: exported app version $ACTUAL_VERSION does not match $VERSION" >&2
	exit 1
}
[ "$ACTUAL_BUILD" = "$BUILD_NUMBER" ] || {
	echo "error: exported app build $ACTUAL_BUILD does not match $BUILD_NUMBER" >&2
	exit 1
}

# --- Embed the Developer ID provisioning profile ------------------------------
# The app needs a keychain access group to reach the data-protection Keychain
# (see VniDropDirect-Signing.entitlements). macOS will not *launch* a binary
# claiming keychain-access-groups unless an embedded provisioning profile
# authorizes it — codesign and the notary service both accept such a binary, but
# launchd then kills it with "Launchd job spawn failed" (POSIX 163). The profile
# is therefore mandatory, not optional.
PROFILE="${DEVELOPER_ID_PROFILE:-$APPLE_DIR/VniDrop/Resources/VniDropDirect.provisionprofile}"
[ -f "$PROFILE" ] || {
	echo "error: Developer ID provisioning profile not found: $PROFILE" >&2
	echo "       Create one (developer.apple.com ▸ Profiles ▸ Developer ID ▸ Application)" >&2
	echo "       for $APP_BUNDLE_ID, or point DEVELOPER_ID_PROFILE at it." >&2
	exit 1
}
echo "==> Embedding provisioning profile"
cp "$PROFILE" "$APP/Contents/embedded.provisionprofile"

echo "==> Enforcing hardened-runtime signature"
# The shipped signature comes from this re-sign, not from the archive, so the
# keychain access group is applied here (see VniDropDirect-Signing.entitlements
# for why it cannot live on the target). codesign does not expand Xcode build
# settings, so resolve the template the same way ExportOptions is resolved
# above — otherwise the group would be signed in as the literal
# "$(DEVELOPMENT_TEAM)…" and the data-protection Keychain would reject the app.
RESOLVED_ENTITLEMENTS="$BUILD_DIR/VniDropDirect.resolved.entitlements"
sed -e "s/\$(DEVELOPMENT_TEAM)/$DEVELOPMENT_TEAM/g" \
	-e "s/\$(PRODUCT_BUNDLE_IDENTIFIER)/$APP_BUNDLE_ID/g" \
	"$APPLE_DIR/VniDrop/Resources/VniDropDirect-Signing.entitlements" > "$RESOLVED_ENTITLEMENTS"
if grep -q '\$(' "$RESOLVED_ENTITLEMENTS"; then
	echo "error: unresolved build settings remain in $RESOLVED_ENTITLEMENTS:" >&2
	grep -n '\$(' "$RESOLVED_ENTITLEMENTS" >&2
	exit 1
fi
# An entitlements file Xcode cannot parse is silently treated as empty, which is
# how the 0.3.1 build lost its entitlements without failing. Lint it explicitly.
plutil -lint "$RESOLVED_ENTITLEMENTS" >/dev/null || {
	echo "error: resolved entitlements are not a valid plist: $RESOLVED_ENTITLEMENTS" >&2
	exit 1
}
"$SCRIPT_DIR/sign-exported-app.sh" \
	"$APP" \
	"$DEVELOPER_ID_APP" \
	"$RESOLVED_ENTITLEMENTS"

# The 0.3.1 direct build shipped with an empty entitlements dict, which silently
# broke every keychain read and write. Assert the group is really in the
# signature so that failure mode can never ship again. Shared with the App Store
# and iOS builds (apple/ci_scripts/ci_post_xcodebuild.sh) so both channels are held to
# the same contract.
echo "==> Verifying keychain access group"
EXPECTED_GROUP="$DEVELOPMENT_TEAM.$APP_BUNDLE_ID"
"$SCRIPT_DIR/verify-keychain-access-group.sh" "$APP" "$APP_BUNDLE_ID"
# A group without a surviving profile is the unlaunchable combination; codesign
# drops unsealed files, so confirm the profile is still there after signing.
[ -f "$APP/Contents/embedded.provisionprofile" ] || {
	echo "error: embedded.provisionprofile missing after signing; the app would" >&2
	echo "       claim $EXPECTED_GROUP without authorization and fail to launch." >&2
	exit 1
}
# The profile authorizes the entitlement only for the certificates it embeds. A
# profile built against a *different* Developer ID cert still archives, signs and
# notarizes cleanly, then dies at launch with "Launchd job spawn failed" — the
# 0.3.1 investigation lost a full cycle to exactly that. Compare the cert that
# actually signed the app against the profile's cert list, since neither the
# team id nor the certificate name distinguishes them.
echo "==> Verifying signing certificate is authorized by the profile"
rm -f "$BUILD_DIR"/signingcert*
codesign -d --extract-certificates="$BUILD_DIR/signingcert" "$APP" 2>/dev/null
SIGNING_SERIAL="$(openssl x509 -inform DER -in "$BUILD_DIR/signingcert0" -noout -serial \
	| cut -d= -f2)"
PROFILE_SERIALS="$(security cms -D -i "$PROFILE" 2>/dev/null | python3 -c '
import plistlib, subprocess, sys
profile = plistlib.loads(sys.stdin.buffer.read())
for cert in profile.get("DeveloperCertificates", []):
    result = subprocess.run(
        ["openssl", "x509", "-inform", "DER", "-noout", "-serial"],
        input=cert, capture_output=True,
    )
    print(result.stdout.decode().strip().split("=")[1])
')"
case " $PROFILE_SERIALS " in
	*" $SIGNING_SERIAL "*) ;;
	*)
		echo "error: the signing certificate is not authorized by the profile." >&2
		echo "       signed with:      $SIGNING_SERIAL" >&2
		echo "       profile allows:   ${PROFILE_SERIALS:-<none>}" >&2
		echo "       The app would notarize but fail to launch. Rebuild the" >&2
		echo "       Developer ID profile against the signing certificate." >&2
		exit 1
		;;
esac

echo "    group:    $EXPECTED_GROUP"
echo "    profile:  $PROFILE"
echo "    cert:     $SIGNING_SERIAL (authorized)"

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
	NOTARY_LOG="$DIST_DIR/$APP_NAME-$VERSION.notary-log.json"
	"$SCRIPT_DIR/notarize.sh" "$DMG" "$NOTARY_PROFILE" "$NOTARY_LOG"
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
jq -n \
	--arg productVersion "$VERSION" \
	--arg directBuildNumber "$BUILD_NUMBER" \
	--arg artifact "$(basename "$DMG")" \
	'{
		productVersion: $productVersion,
		directBuildNumber: $directBuildNumber,
		distribution: "direct",
		artifact: $artifact
	}' > "$BUILD_METADATA"
echo "==> Done."
echo "    dmg:     $DMG"
echo "    version: $VERSION"
echo "    build:   $BUILD_NUMBER"
echo "    size:    $SIZE bytes"
