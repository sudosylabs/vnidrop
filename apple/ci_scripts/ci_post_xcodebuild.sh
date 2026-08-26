#!/bin/bash
#
# Xcode Cloud post-build step.
#
# Verifies that the archived app can actually reach the data-protection Keychain
# before it can go to TestFlight or the App Store. The Rust core keeps the
# endpoint identity there, and an app that resolves to no keychain access group
# gets errSecMissingEntitlement (-34018) on every read and write — which the app
# surfaces as a dead startup screen.
#
# This exists because that failure is invisible to every other check: VniDrop
# 0.3.1 shipped it on the direct-download channel after archiving, signing,
# notarizing and stapling without a single warning. The App Store and iOS builds
# get their access group implicitly, from the provisioning profile's
# application-identifier rather than an explicit entitlement, so a profile or
# capability regression would break them the same silent way.
#
# Xcode Cloud sets CI_ARCHIVE_PATH for archive actions only; for test/analyze
# builds there is nothing to check and the script exits cleanly.
set -euo pipefail

REPO_ROOT="${CI_PRIMARY_REPOSITORY_PATH:-$(cd "$(dirname "$0")/../.." && pwd)}"
cd "$REPO_ROOT"

if [ -z "${CI_ARCHIVE_PATH:-}" ]; then
	echo "==> No archive in this action; skipping keychain access group check."
	exit 0
fi

APP="$(find "$CI_ARCHIVE_PATH/Products/Applications" -maxdepth 1 -name '*.app' | head -1)"
if [ -z "$APP" ]; then
	echo "error: no .app found in $CI_ARCHIVE_PATH" >&2
	exit 1
fi

echo "==> Verifying keychain access group for $(basename "$APP")"
apple/scripts/verify-keychain-access-group.sh "$APP"
