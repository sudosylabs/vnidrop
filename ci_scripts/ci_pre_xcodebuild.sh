#!/bin/bash
#
# Xcode Cloud pre-build step: enable code signing for archive actions.
#
# apple/Signing.xcconfig — included by every target's config file — pins
# CODE_SIGNING_ALLOWED = NO, because local development and PR builds are
# intentionally unsigned. An Xcode Cloud archive must be signed (Xcode Cloud
# issues the certificate and profile itself), so an unsigned archive is useless:
# it cannot go to TestFlight or the App Store.
#
# Signing.xcconfig ends with `#include? "Local.xcconfig"`, and in an xcconfig the
# last assignment wins, so writing that (gitignored) file here overrides the two
# settings without touching the committed one. Xcode Cloud supplies
# DEVELOPMENT_TEAM and the profile through automatic signing; we only re-open the
# gate.
#
# Non-archive actions (build, test, analyze) keep the unsigned fast path.
set -euo pipefail

REPO_ROOT="${CI_PRIMARY_REPOSITORY_PATH:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$REPO_ROOT"

if [ "${CI_XCODEBUILD_ACTION:-}" != "archive" ]; then
	echo "==> Action '${CI_XCODEBUILD_ACTION:-unknown}' is not an archive; leaving builds unsigned."
	exit 0
fi

echo "==> Enabling code signing for the archive (apple/Local.xcconfig)"
# Append rather than overwrite: Local.xcconfig is gitignored so a fresh Xcode
# Cloud checkout never has one, but on a developer machine it carries
# DEVELOPMENT_TEAM and must survive. Last assignment wins either way.
cat >> apple/Local.xcconfig <<'EOF'

// Appended by ci_scripts/ci_pre_xcodebuild.sh — Xcode Cloud archive builds.
// Overrides the unsigned defaults in Signing.xcconfig.
CODE_SIGNING_ALLOWED = YES
CODE_SIGNING_REQUIRED = YES
EOF
