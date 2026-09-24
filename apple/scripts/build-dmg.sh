#!/usr/bin/env bash
#
# Builds the direct-download macOS artifact: a Developer ID–signed, notarized
# .dmg of the VniDropDirect target (the Sparkle-enabled build). Produces:
#   - apple/dist/VniDrop-<version>.dmg            Apple Silicon, signed + stapled
#   - apple/dist/VniDrop-<version>-x86_64.dmg    Intel, signed + stapled
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
# This is the only Rust build in the release: the core it produces is published
# as VnidropCore-<version>.zip, and the App Store workflow downloads that asset
# rather than building its own.
echo "==> Building Rust core (release, Apple Silicon + Intel)"
VNIDROP_APPLE_INTEL=1 CARGO_PROFILE_RELEASE_LTO=false "$SCRIPT_DIR/build-core.sh" release
echo "==> Regenerating Xcode project"
"$VERSION_CONFIG_GENERATOR" all
# AppConfig.swift is gitignored codegen — a clean CI checkout has none.
"$APP_CONFIG_GENERATOR"
( cd "$APPLE_DIR" && xcodegen generate >/dev/null )

rm -rf "$BUILD_DIR" && mkdir -p "$BUILD_DIR" "$DIST_DIR"

# One thin app per architecture. ARCHS stays a single value so Xcode cannot
# lipo the two xcframework slices into a universal download.
build_direct_dmg() {
	local arch="$1"
	local feed_url="$2"
	local dmg_basename="$3"
	local work="$BUILD_DIR/$arch"
	local archive="$work/$APP_NAME.xcarchive"
	local export_dir="$work/export"
	local export_opts="$work/ExportOptions.plist"
	local app resolved_entitlements expected_group signing_serial profile_serials
	local dmg staging notary_log metadata arches actual_version actual_build actual_feed

	rm -rf "$work" && mkdir -p "$work"
	echo "==> Archiving $SCHEME ($CONFIG, $arch)"
	xcodebuild -quiet archive \
		-project "$PROJECT" \
		-scheme "$SCHEME" \
		-configuration "$CONFIG" \
		-destination 'generic/platform=macOS' \
		-archivePath "$archive" \
		ARCHS="$arch" \
		ONLY_ACTIVE_ARCH=NO \
		SU_FEED_URL="$feed_url" \
		MARKETING_VERSION="$VERSION" \
		DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM" \
		CODE_SIGN_STYLE=Manual \
		CODE_SIGN_IDENTITY="$DEVELOPER_ID_APP"
	[ -d "$archive" ] || { echo "error: archive failed ($arch)" >&2; exit 1; }

	echo "==> Exporting Developer ID app ($arch)"
	sed "s/\${DEVELOPMENT_TEAM}/$DEVELOPMENT_TEAM/" \
		"$SCRIPT_DIR/ExportOptions-DeveloperID.plist" > "$export_opts"
	xcodebuild -exportArchive \
		-archivePath "$archive" \
		-exportPath "$export_dir" \
		-exportOptionsPlist "$export_opts"
	app="$export_dir/$APP_NAME.app"
	[ -d "$app" ] || { echo "error: export failed ($arch)" >&2; exit 1; }
	actual_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
		"$app/Contents/Info.plist")"
	actual_build="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' \
		"$app/Contents/Info.plist")"
	actual_feed="$(/usr/libexec/PlistBuddy -c 'Print :SUFeedURL' \
		"$app/Contents/Info.plist")"
	[ "$actual_version" = "$VERSION" ] || {
		echo "error: exported app version $actual_version does not match $VERSION" >&2
		exit 1
	}
	[ "$actual_build" = "$BUILD_NUMBER" ] || {
		echo "error: exported app build $actual_build does not match $BUILD_NUMBER" >&2
		exit 1
	}
	[ "$actual_feed" = "$feed_url" ] || {
		echo "error: exported $arch app feed is '$actual_feed'" >&2
		exit 1
	}
	arches="$(lipo -archs "$app/Contents/MacOS/$APP_NAME")"
	[ "$arches" = "$arch" ] || {
		echo "error: $dmg_basename contains architectures '$arches', expected $arch" >&2
		exit 1
	}

	# The app needs a keychain access group to reach the data-protection Keychain
	# (see VniDropDirect-Signing.entitlements). macOS will not *launch* a binary
	# claiming keychain-access-groups unless an embedded provisioning profile
	# authorizes it — codesign and the notary service both accept such a binary, but
	# launchd then kills it with "Launchd job spawn failed" (POSIX 163). The profile
	# is therefore mandatory, not optional.
	local profile="${DEVELOPER_ID_PROFILE:-$APPLE_DIR/VniDrop/Resources/VniDropDirect.provisionprofile}"
	[ -f "$profile" ] || {
		echo "error: Developer ID provisioning profile not found: $profile" >&2
		echo "       Create one (developer.apple.com ▸ Profiles ▸ Developer ID ▸ Application)" >&2
		echo "       for $APP_BUNDLE_ID, or point DEVELOPER_ID_PROFILE at it." >&2
		exit 1
	}
	echo "==> Embedding provisioning profile"
	cp "$profile" "$app/Contents/embedded.provisionprofile"

	echo "==> Enforcing hardened-runtime signature"
	# The shipped signature comes from this re-sign, not from the archive, so the
	# keychain access group is applied here (see VniDropDirect-Signing.entitlements
	# for why it cannot live on the target). codesign does not expand Xcode build
	# settings, so resolve the template the same way ExportOptions is resolved
	# above — otherwise the group would be signed in as the literal
	# "$(DEVELOPMENT_TEAM)…" and the data-protection Keychain would reject the app.
	resolved_entitlements="$work/VniDropDirect.resolved.entitlements"
	sed -e "s/\$(DEVELOPMENT_TEAM)/$DEVELOPMENT_TEAM/g" \
		-e "s/\$(PRODUCT_BUNDLE_IDENTIFIER)/$APP_BUNDLE_ID/g" \
		"$APPLE_DIR/VniDrop/Resources/VniDropDirect-Signing.entitlements" > "$resolved_entitlements"
	if grep -q '\$(' "$resolved_entitlements"; then
		echo "error: unresolved build settings remain in $resolved_entitlements:" >&2
		grep -n '\$(' "$resolved_entitlements" >&2
		exit 1
	fi
	# An entitlements file Xcode cannot parse is silently treated as empty, which is
	# how the 0.3.1 build lost its entitlements without failing. Lint it explicitly.
	plutil -lint "$resolved_entitlements" >/dev/null || {
		echo "error: resolved entitlements are not a valid plist: $resolved_entitlements" >&2
		exit 1
	}
	"$SCRIPT_DIR/sign-exported-app.sh" \
		"$app" \
		"$DEVELOPER_ID_APP" \
		"$resolved_entitlements"

	# The 0.3.1 direct build shipped with an empty entitlements dict, which silently
	# broke every keychain read and write. Assert the group is really in the
	# signature so that failure mode can never ship again. Shared with the App Store
	# and iOS builds (apple/ci_scripts/ci_post_xcodebuild.sh) so both channels are held to
	# the same contract.
	echo "==> Verifying keychain access group"
	expected_group="$DEVELOPMENT_TEAM.$APP_BUNDLE_ID"
	"$SCRIPT_DIR/verify-keychain-access-group.sh" "$app" "$APP_BUNDLE_ID"
	# A group without a surviving profile is the unlaunchable combination; codesign
	# drops unsealed files, so confirm the profile is still there after signing.
	[ -f "$app/Contents/embedded.provisionprofile" ] || {
		echo "error: embedded.provisionprofile missing after signing; the app would" >&2
		echo "       claim $expected_group without authorization and fail to launch." >&2
		exit 1
	}
	# The profile authorizes the entitlement only for the certificates it embeds. A
	# profile built against a *different* Developer ID cert still archives, signs and
	# notarizes cleanly, then dies at launch with "Launchd job spawn failed" — the
	# 0.3.1 investigation lost a full cycle to exactly that. Compare the cert that
	# actually signed the app against the profile's cert list, since neither the
	# team id nor the certificate name distinguishes them.
	echo "==> Verifying signing certificate is authorized by the profile"
	rm -f "$work"/signingcert*
	codesign -d --extract-certificates="$work/signingcert" "$app" 2>/dev/null
	signing_serial="$(openssl x509 -inform DER -in "$work/signingcert0" -noout -serial \
		| cut -d= -f2)"
	profile_serials="$(security cms -D -i "$profile" 2>/dev/null | python3 -c '
import plistlib, subprocess, sys
profile = plistlib.loads(sys.stdin.buffer.read())
for cert in profile.get("DeveloperCertificates", []):
    result = subprocess.run(
        ["openssl", "x509", "-inform", "DER", "-noout", "-serial"],
        input=cert, capture_output=True,
    )
    print(result.stdout.decode().strip().split("=")[1])
')"
	case " $profile_serials " in
		*" $signing_serial "*) ;;
		*)
			echo "error: the signing certificate is not authorized by the profile." >&2
			echo "       signed with:      $signing_serial" >&2
			echo "       profile allows:   ${profile_serials:-<none>}" >&2
			echo "       The app would notarize but fail to launch. Rebuild the" >&2
			echo "       Developer ID profile against the signing certificate." >&2
			exit 1
			;;
	esac

	echo "    group:    $expected_group"
	echo "    profile:  $profile"
	echo "    cert:     $signing_serial (authorized)"

	dmg="$DIST_DIR/$dmg_basename"
	rm -f "$dmg"
	staging="$work/dmg-staging"
	rm -rf "$staging" && mkdir -p "$staging"
	cp -R "$app" "$staging/"
	ln -s /Applications "$staging/Applications"

	echo "==> Building DMG ($arch)"
	if command -v create-dmg >/dev/null 2>&1; then
		create-dmg \
			--volname "$APP_NAME" \
			--app-drop-link 380 205 \
			--icon "$APP_NAME.app" 130 205 \
			--window-size 540 380 \
			--no-internet-enable \
			"$dmg" "$staging" >/dev/null || {
				# create-dmg exits non-zero if it can't set the fancy layout; fall back.
				[ -f "$dmg" ] || hdiutil create -volname "$APP_NAME" -srcfolder "$staging" \
					-ov -format UDZO "$dmg" >/dev/null
			}
	else
		hdiutil create -volname "$APP_NAME" -srcfolder "$staging" \
			-ov -format UDZO "$dmg" >/dev/null
	fi

	echo "==> Signing DMG ($arch)"
	codesign --force --sign "$DEVELOPER_ID_APP" --timestamp "$dmg"

	if [ -n "${NOTARY_PROFILE:-}" ]; then
		echo "==> Notarizing $dmg_basename (profile: $NOTARY_PROFILE)"
		notary_log="$DIST_DIR/${dmg_basename%.dmg}.notary-log.json"
		"$SCRIPT_DIR/notarize.sh" "$dmg" "$NOTARY_PROFILE" "$notary_log"
		echo "==> Stapling"
		xcrun stapler staple "$dmg"
		xcrun stapler validate "$dmg"
		spctl -a -vvv --type install "$dmg" || true
	else
		echo "==> NOTARY_PROFILE unset — skipping notarization of $dmg_basename."
		echo "    The DMG is signed but NOT notarized; Gatekeeper will block it until"
		echo "    you run 'xcrun notarytool store-credentials' and re-run with NOTARY_PROFILE set."
	fi

	metadata="$DIST_DIR/${dmg_basename%.dmg}.build-info.json"
	jq -n \
		--arg productVersion "$VERSION" \
		--arg directBuildNumber "$BUILD_NUMBER" \
		--arg artifact "$dmg_basename" \
		'{
			productVersion: $productVersion,
			directBuildNumber: $directBuildNumber,
			distribution: "direct",
			artifact: $artifact
		}' > "$metadata"
	echo "==> Done ($arch)."
	echo "    dmg:     $dmg"
	echo "    version: $VERSION"
	echo "    build:   $BUILD_NUMBER"
	echo "    size:    $(stat -f%z "$dmg") bytes"
}

ARM_FEED="https://github.com/sudosylabs/vnidrop/releases/latest/download/appcast.xml"
INTEL_FEED="https://github.com/sudosylabs/vnidrop/releases/latest/download/appcast-x86_64.xml"
build_direct_dmg arm64 "$ARM_FEED" "$APP_NAME-$VERSION.dmg"
build_direct_dmg x86_64 "$INTEL_FEED" "$APP_NAME-$VERSION-x86_64.dmg"
