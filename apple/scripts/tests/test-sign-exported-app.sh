#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
sign_exported_app="$script_dir/../sign-exported-app.sh"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/vnidrop-codesign-test.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

mkdir -p "$scratch/bin" "$scratch/VniDrop.app/Contents/MacOS"
app="$scratch/VniDrop.app"
entitlements="$scratch/VniDropDirect.entitlements"
calls="$scratch/calls.txt"
printf '<plist><dict/></plist>\n' > "$entitlements"
printf 'binary\n' > "$app/Contents/MacOS/VniDrop"

cat > "$scratch/bin/codesign" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >> "$FAKE_CODESIGN_CALLS"
case " $* " in
	*" --display "*)
		if [[ ${FAKE_CODESIGN_MODE:-runtime} == missing-runtime ]]; then
			printf '%s\n' \
				'CodeDirectory v=20500 size=123 flags=0x0(none) hashes=1+0 location=embedded' \
				>&2
		else
			printf '%s\n' \
				'CodeDirectory v=20500 size=123 flags=0x10000(runtime) hashes=1+0 location=embedded' \
				>&2
		fi
		;;
	*" --verify "*)
		if [[ ${FAKE_CODESIGN_MODE:-runtime} == verify-error ]]; then
			printf '%s\n' 'invalid signature' >&2
			exit 1
		fi
		;;
esac
SCRIPT
chmod +x "$scratch/bin/codesign"

PATH="$scratch/bin:$PATH" \
	FAKE_CODESIGN_CALLS="$calls" \
	"$sign_exported_app" \
	"$app" \
	'Developer ID Application: Example (ABCDEFGHIJ)' \
	"$entitlements" >/dev/null
grep -F -- \
	'--force --sign Developer ID Application: Example (ABCDEFGHIJ) --options runtime --timestamp --entitlements' \
	"$calls" >/dev/null
grep -F -- '--verify --deep --strict --verbose=2' "$calls" >/dev/null
grep -F -- '--display --verbose=4' "$calls" >/dev/null

if PATH="$scratch/bin:$PATH" \
	FAKE_CODESIGN_CALLS="$calls" \
	FAKE_CODESIGN_MODE=missing-runtime \
	"$sign_exported_app" \
	"$app" \
	'Developer ID Application: Example (ABCDEFGHIJ)' \
	"$entitlements" >/dev/null 2>&1; then
	printf 'A signature without the hardened runtime must fail\n' >&2
	exit 1
fi

if PATH="$scratch/bin:$PATH" \
	FAKE_CODESIGN_CALLS="$calls" \
	FAKE_CODESIGN_MODE=verify-error \
	"$sign_exported_app" \
	"$app" \
	'Developer ID Application: Example (ABCDEFGHIJ)' \
	"$entitlements" >/dev/null 2>&1; then
	printf 'Signature verification errors must fail\n' >&2
	exit 1
fi

printf 'Exported app signing tests passed.\n'
