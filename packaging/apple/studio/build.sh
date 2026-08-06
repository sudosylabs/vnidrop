#!/usr/bin/env bash
# Render all (locale x screen) App Store screenshots with Typst.
#
#   ./build.sh                 # -> generated/<Language>/<Name>.png   (safe default)
#   ./build.sh --publish       # -> ../<Language>/<Name>.png          (ships to App Store)
#   LOCALES="fr de" SCREENS="share-securely" ./build.sh   # subset
#
# Screenshots are read from assets/shots/<locale>/<screen>.png when present
# (pass nothing and you get the "screenshot here" placeholder).
set -euo pipefail
cd "$(dirname "$0")"

PUBLISH=""; [[ "${1:-}" == "--publish" ]] && PUBLISH=1

LOCALES=${LOCALES:-"en fr de es it nl pl pt ru"}
SCREENS=${SCREENS:-"choose-receivers send-anywhere share-securely stay-private"}

# locale code -> output folder name (matches existing packaging/apple/<Language>/)
folder_for() { case "$1" in
  en) echo English;; fr) echo French;; de) echo German;; es) echo Spanish;;
  it) echo Italian;; nl) echo Dutch;; pl) echo Polish;; pt) echo Portuguese;; ru) echo Russian;;
  *) echo "$1";; esac; }

# screen id -> output file basename (matches existing filenames)
name_for() { case "$1" in
  choose-receivers) echo "Choose Receivers";; send-anywhere) echo "Send Anywhere";;
  share-securely) echo "Share Securely";; stay-private) echo "Stay private";;
  *) echo "$1";; esac; }

# screen id -> device mockup mode (which mockup frames the screenshot), or empty for none
mode_for() { case "$1" in
  send-anywhere) echo "rotated";; choose-receivers|share-securely) echo "straight";;
  *) echo "";; esac; }

n=0
for loc in $LOCALES; do
  folder=$(folder_for "$loc")
  outdir=$([[ -n "$PUBLISH" ]] && echo "../$folder" || echo "generated/$folder")
  mkdir -p "$outdir"
  for scr in $SCREENS; do
    shot="assets/shots/$loc/$scr.png"
    mode="$(mode_for "$scr")"

    # Frame the screenshot into its mockup (frame.sh masks it to the real screen
    # shape) when a capture and a mockup mode exist; regenerate if the shot is newer.
    device_arg="none"
    if [[ -n "$mode" && -f "$shot" ]]; then
      device="assets/shots/$loc/$scr.device.png"
      [[ ! -f "$device" || "$shot" -nt "$device" ]] && ./frame.sh "$mode" "$shot" "$device" >/dev/null
      device_arg="$device"
    fi

    out="$outdir/$(name_for "$scr").png"
    typst compile screens.typ "$out" \
      --input locale="$loc" --input screen="$scr" --input device="$device_arg" --ppi 72
    echo "  ✅ $out"
    n=$((n+1))
  done
done
echo ""
echo "Done — $n screenshot(s)$([[ -n "$PUBLISH" ]] && echo ' (published)')."
