#!/usr/bin/env bash
# Capture real, localized app screens for the studio pipeline.
#
# Drives the app straight into each marketing screen via the DEBUG `-VniScreenshot`
# launch argument (no UI automation needed), once per locale via `-AppleLanguages`,
# and writes assets/shots/<locale>/<screen>.png at the device's native resolution.
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
DEVICE="${SCREENSHOT_DEVICE:-iPhone 17 Pro Max}"
BUNDLE_ID="com.vnidrop.app"
LOCALES=${LOCALES:-"en fr de es it nl pl pt ru"}

# scenario (launch arg value) -> studio screen id (output filename stem)
SCENARIOS="share:share-securely approval:choose-receivers transfer-details:send-anywhere"

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
	mkdir -p "assets/shots/$loc"
	for pair in $SCENARIOS; do
		scenario="${pair%%:*}"; screen="${pair##*:}"
		xcrun simctl launch --terminate-running-process "$UDID" "$BUNDLE_ID" \
			-VniScreenshot "$scenario" -AppleLanguages "($loc)" -AppleLocale "$loc" >/dev/null
		sleep 4
		# simctl can't write into the project tree (TCC blocks the CoreSimulator helper
		# on external/again-protected volumes), so capture to a temp file and move it in.
		tmp="$(mktemp -t vnishot).png"
		xcrun simctl io "$UDID" screenshot "$tmp" >/dev/null 2>&1
		mv "$tmp" "assets/shots/$loc/$screen.png"
		echo "  📸 $loc/$screen.png"
	done
done
echo ""
echo "Done. Now run ./build.sh to composite."
