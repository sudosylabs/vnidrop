#!/usr/bin/env bash
# Composite a screenshot into a device mockup using the mockup's real screen shape
# as the mask — so the screen curvature comes from the mockup, never a guessed radius.
# Output is a device PNG the size of the mockup, transparent outside the phone, for
# Typst to place directly.
#
#   ./frame.sh straight assets/shots/en/share-securely.png out-device.png
#   ./frame.sh rotated  assets/shots/en/send-anywhere.png  out-device.png
set -euo pipefail
cd "$(dirname "$0")"

MODE="$1"; SHOT="$2"; OUT="$3"
MOCKUP="assets/mockup-$MODE.png"
MASK="assets/mask-$MODE.png"

# Cache the screen-glass mask (near-black region of the mockup, flattened off transparency).
if [[ ! -f "$MASK" || "$MOCKUP" -nt "$MASK" ]]; then
	magick "$MOCKUP" -background magenta -flatten -colorspace Gray -threshold 6% -negate "$MASK"
fi

CW=$(magick identify -format "%w" "$MOCKUP"); CH=$(magick identify -format "%h" "$MOCKUP")
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

if [[ "$MODE" == "rotated" ]]; then
	# Perspective-warp onto the tilted glass quad (canvas-sized already).
	./warp.sh "$SHOT" "$tmp/screen.png" >/dev/null
else
	# Cover-fit the screenshot to the screen's bounding box, placed at its offset.
	read SW SH SX SY <<<"$(magick "$MASK" -format "%@" info: | tr 'x+' ' ')"
	magick "$SHOT" -resize "${SW}x${SH}^" -gravity center -extent "${SW}x${SH}" "$tmp/fit.png"
	magick -size "${CW}x${CH}" xc:none "$tmp/fit.png" -geometry "+${SX}+${SY}" -composite "$tmp/screen.png"
fi

# Clip the screen layer to the exact glass shape (rounded corners from the mask),
# then lay it over the mockup so the bezel frames it.
magick "$tmp/screen.png" "$MASK" -alpha off -compose CopyOpacity -composite "$tmp/clipped.png"
magick "$MOCKUP" "$tmp/clipped.png" -compose over -composite "$OUT"
echo "framed -> $OUT"
