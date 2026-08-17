#!/usr/bin/env bash
# Capture real, localized app screens for the studio pipeline.
#
# Drives the app straight into each marketing screen via the DEBUG `-VniScreenshot`
# launch argument (no UI automation needed), once per locale via `-AppleLanguages`,
# and writes generated/shots/<platform>/<locale>/<screen>.png at the device's native resolution.
#
#   ./capture.sh                      # all locales
#   LOCALES="en fr" ./capture.sh
#   SCREENSHOT_DEVICE="iPhone 17 Pro Max" ./capture.sh
#
# Requires a Debug build (the fixture gateway is compiled under #if DEBUG).
set -euo pipefail
trap 'echo "capture.sh: failed (rc=$?) at line $LINENO" >&2' ERR
cd "$(dirname "$0")"

APPLE_DIR="$(cd ../../../apple && pwd)"
BUNDLE_ID="com.vnidrop.app"
LOCALES=${LOCALES:-"en fr de es it nl pl pt ru"}

# PLATFORM=iphone|ipad selects the simulator and the shots subdir (must match main.swift).
PLATFORM="${PLATFORM:-iphone}"
case "$PLATFORM" in
	ipad) DEVICE="${SCREENSHOT_DEVICE:-iPad Pro 13-inch (M5)}";;
	*)    DEVICE="${SCREENSHOT_DEVICE:-iPhone 17 Pro Max}";;
esac

# scenario (launch arg value) -> studio screen id (output filename stem).
# send-anywhere reuses the share screenshot (see ScreenSpec shotId), so it isn't
# captured separately; stay-private uses black screens (no capture).
SCENARIOS="share:share-securely approval:choose-receivers"

# Screenshots are transient build output, regenerated per run — never committed. They
# live under generated/ (git-ignored), not in assets/.
SHOTS_DIR="${SHOTS_DIR:-generated/shots/$PLATFORM}"

# ---- macOS: no simulator. Build the native app, drive it with the fixture args, size its
# window to 16:10 and grab it with screencapture. Needs Accessibility + Screen Recording
# permission granted to the terminal (you'll be prompted the first time).
if [ "$PLATFORM" = "mac" ]; then
	WIN_X=60; WIN_Y=80; WIN_W=1440; WIN_H=900   # 16:10, matches the MacBook screen aspect
	echo "==> Regenerating project"; (cd "$APPLE_DIR" && xcodegen generate >/dev/null)
	echo "==> Building VniDrop (Debug) for macOS"
	xcodebuild build -project "$APPLE_DIR/VniDrop.xcodeproj" -scheme VniDrop \
		-destination 'platform=macOS' -configuration Debug CODE_SIGNING_ALLOWED=NO >/dev/null
	APP="$(xcodebuild -project "$APPLE_DIR/VniDrop.xcodeproj" -scheme VniDrop \
		-destination 'platform=macOS' -configuration Debug -showBuildSettings 2>/dev/null \
		| awk -F' = ' '/ BUILT_PRODUCTS_DIR /{print $2; exit}')/VniDrop.app"
	EXE_NAME="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist" 2>/dev/null || echo VniDrop)"
	EXE="$APP/Contents/MacOS/$EXE_NAME"
	echo "    app: $APP  (exe: $EXE_NAME)"
	echo "    (grant Accessibility + Screen Recording to your terminal if prompted)"
	for loc in $LOCALES; do
		mkdir -p "$SHOTS_DIR/$loc"
		for pair in $SCENARIOS; do
			scenario="${pair%%:*}"; screen="${pair##*:}"
			pkill -x "$EXE_NAME" 2>/dev/null || true; sleep 1
			# Launch the binary directly (avoids LaunchServices -1712 for DerivedData apps).
			"$EXE" -VniScreenshot "$scenario" \
				-AppleLanguages "($loc)" -AppleLocale "$loc" -AppleInterfaceStyle Dark &
			sleep 4
			osascript >/dev/null 2>&1 <<-OSA || true
			tell application "System Events" to tell process "$EXE_NAME"
				set frontmost to true
				set position of front window to {$WIN_X, $WIN_Y}
				set size of front window to {$WIN_W, $WIN_H}
			end tell
			OSA
			sleep 1
			tmp="$(mktemp -t vnishot).png"
			screencapture -x -R${WIN_X},${WIN_Y},${WIN_W},${WIN_H} "$tmp"
			mv "$tmp" "$SHOTS_DIR/$loc/$screen.png"
			echo "  🖥️  $SHOTS_DIR/$loc/$screen.png"
		done
	done
	pkill -x "$EXE_NAME" 2>/dev/null || true
	echo ""; echo "Done. Now run 'PLATFORM=mac swift run studio' to composite."
	exit 0
fi

echo "==> Regenerating project"; (cd "$APPLE_DIR" && xcodegen generate >/dev/null)

echo "==> Building VniDrop (Debug) for $DEVICE"
xcodebuild build -project "$APPLE_DIR/VniDrop.xcodeproj" -scheme VniDrop \
	-destination "platform=iOS Simulator,name=$DEVICE" -configuration Debug \
	CODE_SIGNING_ALLOWED=NO >/dev/null
APP="$(xcodebuild -project "$APPLE_DIR/VniDrop.xcodeproj" -scheme VniDrop \
	-destination "platform=iOS Simulator,name=$DEVICE" -configuration Debug \
	-showBuildSettings 2>/dev/null | awk -F' = ' '/ BUILT_PRODUCTS_DIR /{print $2; exit}')/VniDrop.app"

UDID="$(xcrun simctl list devices available | grep -F "$DEVICE (" | head -1 \
	| grep -oiE '[0-9a-f-]{36}' | head -1)"
[ -n "$UDID" ] || { echo "error: no simulator '$DEVICE'"; exit 1; }
echo "    device: $UDID"
echo "    app:    $APP"

xcrun simctl boot "$UDID" 2>/dev/null || true
xcrun simctl bootstatus "$UDID" -b >/dev/null 2>&1 || true
xcrun simctl ui "$UDID" appearance dark >/dev/null 2>&1 || true  # match the dark marketing look
xcrun simctl status_bar "$UDID" override --time "9:41" \
	--batteryState charged --batteryLevel 100 --cellularBars 4 --wifiBars 3 >/dev/null 2>&1 || true
xcrun simctl install "$UDID" "$APP"

for loc in $LOCALES; do
	mkdir -p "$SHOTS_DIR/$loc"
	for pair in $SCENARIOS; do
		scenario="${pair%%:*}"; screen="${pair##*:}"
		xcrun simctl launch --terminate-running-process "$UDID" "$BUNDLE_ID" \
			-VniScreenshot "$scenario" -AppleLanguages "($loc)" -AppleLocale "$loc" >/dev/null
		sleep 4
		# simctl can't write into the project tree (TCC blocks the CoreSimulator helper
		# on external/again-protected volumes), so capture to a temp file and move it in.
		tmp="$(mktemp -t vnishot).png"
		xcrun simctl io "$UDID" screenshot "$tmp" >/dev/null 2>&1
		mv "$tmp" "$SHOTS_DIR/$loc/$screen.png"
		echo "  📸 $SHOTS_DIR/$loc/$screen.png"
	done
done
echo ""
echo "Done. Now run 'swift run studio' to composite."
