#!/usr/bin/env bash
# One-shot: capture fresh per-locale screenshots from the app, then composite every
# marketing screen. Screenshots are transient (generated/shots, git-ignored) and are
# regenerated each run — never committed.
#
#   ./generate.sh                 # capture + composite -> generated/<Language>/
#   ./generate.sh --publish       # capture + composite -> ../<Language>/ (ships)
#   LOCALES="en fr" ./generate.sh # subset of locales
#
# During layout iteration you don't need to re-capture every time: run `./capture.sh`
# once, then `swift run studio` on its own (it reads the existing generated/shots).
set -euo pipefail
cd "$(dirname "$0")"

./capture.sh
swift run studio "$@"
