#!/usr/bin/env bash
#
# Fails if a built .app cannot reach the data-protection Keychain.
#
# The Rust core stores the endpoint identity with kSecUseDataProtectionKeychain
# (crates/vnidrop/src/secure_secret/apple.rs). That keychain only answers to a
# process that resolves to a keychain access group. An app without one gets
# errSecMissingEntitlement (-34018) on every read and write, which surfaces as
# VnidropError::SecureStorageUnavailable and a dead startup screen. VniDrop 0.3.1
# shipped exactly that on macOS: the failure is invisible at build time — it
# archives, signs, notarizes and staples cleanly — so it needs an explicit check.
#
# An app resolves to a group in one of two ways, and both are accepted here:
#   * keychain-access-groups names it (the Developer ID direct build, which has
#     no provisioning profile to supply an application identifier); or
#   * application-identifier is <TEAM>.<bundle id>, whose value doubles as the
#     app's default access group (the App Store / iOS builds).
#
# Usage: verify-keychain-access-group.sh <app-bundle> [bundle-id]
#        bundle-id defaults to PRODUCT_BUNDLE_IDENTIFIER in apple/project.yml.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPLE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

if [ $# -lt 1 ]; then
	echo "usage: $(basename "$0") <app-bundle> [bundle-id]" >&2
	exit 2
fi
APP="$1"
[ -d "$APP" ] || { echo "error: not an app bundle: $APP" >&2; exit 1; }

BUNDLE_ID="${2:-}"
if [ -z "$BUNDLE_ID" ]; then
	BUNDLE_ID="$(sed -nE 's/^[[:space:]]*PRODUCT_BUNDLE_IDENTIFIER:[[:space:]]*(.+)$/\1/p' \
		"$APPLE_DIR/project.yml" | head -1)"
fi
[ -n "$BUNDLE_ID" ] || {
	echo "error: could not determine the bundle id" >&2
	exit 1
}

ENTITLEMENTS="$(codesign -d --entitlements - --xml "$APP" 2>/dev/null || true)"
[ -n "$ENTITLEMENTS" ] || {
	echo "error: $APP has no entitlements; it cannot reach the Keychain." >&2
	exit 1
}
# Unsigned/ad-hoc builds have no team, and the check is meaningless there.
TEAM="$(codesign -dvvv "$APP" 2>&1 | sed -nE 's/^TeamIdentifier=(.+)$/\1/p' | head -1)"
if [ -z "$TEAM" ] || [ "$TEAM" = "not set" ]; then
	echo "note: $APP is not team-signed; skipping keychain access group check."
	exit 0
fi

# Via a file, not a pipe: the heredoc below already occupies stdin.
ENTITLEMENTS_FILE="$(mktemp -t vnidrop-entitlements)"
trap 'rm -f "$ENTITLEMENTS_FILE"' EXIT
printf '%s' "$ENTITLEMENTS" > "$ENTITLEMENTS_FILE"

python3 - "$TEAM" "$BUNDLE_ID" "$APP" "$ENTITLEMENTS_FILE" <<'PY'
import plistlib, sys

team, bundle_id, app, entitlements_file = sys.argv[1:5]
with open(entitlements_file, "rb") as handle:
    raw = handle.read()
try:
    entitlements = plistlib.loads(raw)
except Exception as exc:  # a plist Xcode cannot parse is silently treated as empty
    sys.exit(f"error: could not parse entitlements of {app}: {exc}")

expected = f"{team}.{bundle_id}"
groups = list(entitlements.get("keychain-access-groups", []))
# application-identifier doubles as the default access group, which is how the
# profile-provisioned App Store and iOS builds get one without naming it.
for key in ("com.apple.application-identifier", "application-identifier"):
    value = entitlements.get(key)
    if value and value not in groups:
        groups.append(value)

def covers(group: str) -> bool:
    if group == expected:
        return True
    # A trailing wildcard is what Apple issues in profiles, e.g. "TEAM.*".
    return group.endswith(".*") and expected.startswith(group[:-1])

if not any(covers(g) for g in groups):
    print(f"error: {app} resolves to no usable keychain access group.", file=sys.stderr)
    print(f"       expected:   {expected}", file=sys.stderr)
    print(f"       found:      {groups or '<none>'}", file=sys.stderr)
    print("       The app would build and ship, then fail at startup with", file=sys.stderr)
    print("       SecureStorageUnavailable (errSecMissingEntitlement).", file=sys.stderr)
    sys.exit(1)

print(f"    keychain access group: {expected} (via {[g for g in groups if covers(g)][0]})")
PY
