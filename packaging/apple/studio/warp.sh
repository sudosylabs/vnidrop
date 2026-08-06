#!/usr/bin/env bash
# Perspective-warp a flat screenshot onto the tilted mockup's screen glass.
# Output is a transparent PNG the exact size of mockup-rotated.png, so Typst can
# stack it directly over the mockup (bezel frames it).
#
#   ./warp.sh assets/shots/en/send-anywhere.png assets/shots/en/send-anywhere.warped.png
#
# The 4 destination corners are the glass corners of mockup-rotated.png (1696x2528),
# auto-detected once. Re-detect if the mockup changes (see README).
set -euo pipefail
cd "$(dirname "$0")"

SRC="$1"; OUT="$2"
MW=1696; MH=2528                     # mockup-rotated.png dimensions
TL="136,28"; TR="836,236"; BL="944,2136"; BR="1668,2312"

W=$(magick identify -format "%w" "$SRC")
H=$(magick identify -format "%h" "$SRC")

magick \
  \( -size ${MW}x${MH} xc:none \) \
  \( "$SRC" -virtual-pixel transparent +distort Perspective \
       "0,0 $TL  $((W-1)),0 $TR  0,$((H-1)) $BL  $((W-1)),$((H-1)) $BR" \) \
  -background none -flatten "$OUT"
echo "warped -> $OUT"
