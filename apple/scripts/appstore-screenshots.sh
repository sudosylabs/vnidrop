#!/usr/bin/env bash
#
# Captures App Store screenshots for the VniDrop iOS/iPad app by running the
# VniDropUITests screenshot test on a simulator and extracting the attachments
# at the device's native resolution.
#
# Usage:
#   apple/scripts/appstore-screenshots.sh [output-dir]
#
# Environment:
#   SCREENSHOT_DEVICE   Simulator device name (default: "iPad Pro 13-inch (M5)")
#                       13-inch iPad → 2064×2752, accepted by App Store Connect.
#
# The output directory receives one PNG per tab (01-Send.png, 02-Receive.png, …).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
APPLE_DIR="$REPO_ROOT/apple"

DEVICE="${SCREENSHOT_DEVICE:-iPad Pro 13-inch (M5)}"
OUT_DIR="${1:-$HOME/Desktop/vnidrop-appstore-screenshots}"

echo "==> Regenerating Xcode project (picks up the screenshot target)"
(cd "$APPLE_DIR" && xcodegen generate >/dev/null)

echo "==> Resolving simulator: $DEVICE"
# Match the device name literally (it contains parentheses, e.g. "(M5)"), then
# pull the UUID from the same line.
DEVICE_LINE="$(xcrun simctl list devices available | grep -F "$DEVICE (" | head -1)"
UDID="$(printf '%s' "$DEVICE_LINE" \
	| grep -oiE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)"
if [ -z "${UDID:-}" ]; then
	echo "error: no available simulator named '$DEVICE'." >&2
	echo "       list options with: xcrun simctl list devices available" >&2
	exit 1
fi
echo "    udid: $UDID"

echo "==> Booting simulator"
xcrun simctl boot "$UDID" 2>/dev/null || true
xcrun simctl bootstatus "$UDID" -b >/dev/null 2>&1 || true

# Clean marketing status bar (Apple's 9:41, full signal/battery).
xcrun simctl status_bar "$UDID" override \
	--time "9:41" \
	--batteryState charged --batteryLevel 100 \
	--cellularMode active --cellularBars 4 \
	--wifiMode active --wifiBars 3 >/dev/null 2>&1 || true

RESULT_DIR="$(mktemp -d)"
RESULT="$RESULT_DIR/screenshots.xcresult"
ATT_DIR="$RESULT_DIR/attachments"
trap 'rm -rf "$RESULT_DIR"' EXIT

echo "==> Running UI screenshot test (this builds and launches the app)"
xcodebuild test \
	-project "$APPLE_DIR/VniDrop.xcodeproj" \
	-scheme VniDropScreenshots \
	-destination "platform=iOS Simulator,id=$UDID" \
	-resultBundlePath "$RESULT" \
	-only-testing:VniDropUITests \
	CODE_SIGNING_ALLOWED=NO \
	| tail -12

echo "==> Extracting screenshots"
xcrun xcresulttool export attachments --path "$RESULT" --output-path "$ATT_DIR"

mkdir -p "$OUT_DIR"
python3 - "$ATT_DIR" "$OUT_DIR" <<'PY'
import json, os, re, shutil, sys

att_dir, out_dir = sys.argv[1], sys.argv[2]
manifest = os.path.join(att_dir, "manifest.json")
with open(manifest) as f:
	data = json.load(f)

# Xcode suffixes attachment names with "_<n>_<uuid>.png"; keep only "NN-Name".
pattern = re.compile(r"^(\d\d-[A-Za-z]+)")
count = 0

def walk(node):
	global count
	if isinstance(node, dict):
		name = node.get("suggestedHumanReadableName")
		src = node.get("exportedFileName")
		m = pattern.match(name) if name else None
		if m and src:
			base = f"{m.group(1)}.png"
			shutil.copyfile(os.path.join(att_dir, src), os.path.join(out_dir, base))
			print(f"    {base}")
			count += 1
		for v in node.values():
			walk(v)
	elif isinstance(node, list):
		for v in node:
			walk(v)

walk(data)
if count == 0:
	sys.exit("error: no named screenshots found in the result bundle")
PY

echo "==> Done. Screenshots in: $OUT_DIR"
ls -1 "$OUT_DIR"
