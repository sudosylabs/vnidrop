#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
verifier="$script_dir/../verify-apk-signature.sh"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/vnidrop-apksigner-test.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

apk="$scratch/app.apk"
fake_apksigner="$scratch/apksigner"
printf 'apk\n' > "$apk"

cat > "$fake_apksigner" <<'SCRIPT'
#!/usr/bin/env bash
case "${FAKE_APKSIGNER_MODE:-success}" in
	success)
		printf '%s\n' \
			'Verifies' \
			'Signer #1 certificate SHA-256 digest: AA:BB:CC:DD' >&2
		;;
	missing)
		printf '%s\n' 'Verifies' >&2
		;;
	failure)
		printf '%s\n' 'invalid APK signature' >&2
		exit 1
		;;
esac
SCRIPT
chmod +x "$fake_apksigner"

actual="$(
	APKSIGNER="$fake_apksigner" \
		"$verifier" "$apk" "aa bb cc dd"
)"
[[ $actual == aabbccdd ]]

if APKSIGNER="$fake_apksigner" \
	"$verifier" "$apk" deadbeef >/dev/null 2>&1; then
	printf 'Expected a certificate mismatch to fail\n' >&2
	exit 1
fi

if FAKE_APKSIGNER_MODE=missing APKSIGNER="$fake_apksigner" \
	"$verifier" "$apk" aabbccdd >/dev/null 2>&1; then
	printf 'Expected missing certificate output to fail\n' >&2
	exit 1
fi

if FAKE_APKSIGNER_MODE=failure APKSIGNER="$fake_apksigner" \
	"$verifier" "$apk" aabbccdd >/dev/null 2>&1; then
	printf 'Expected signature verification failure to propagate\n' >&2
	exit 1
fi

printf 'APK signature verifier tests passed.\n'
