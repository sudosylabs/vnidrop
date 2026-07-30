#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
notarize="$script_dir/../notarize.sh"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/vnidrop-notarize-test.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

mkdir -p "$scratch/bin"
artifact="$scratch/VniDrop.dmg"
calls="$scratch/calls.txt"
log_output="$scratch/notary/notary-log.json"
printf 'dmg\n' > "$artifact"

cat > "$scratch/bin/xcrun" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >> "$FAKE_NOTARY_CALLS"
if [[ $1 == notarytool && $2 == submit ]]; then
	case "${FAKE_NOTARY_MODE:-accepted}" in
		accepted)
			printf '%s\n' \
				'{"id":"11111111-1111-1111-1111-111111111111","status":"Accepted"}'
			;;
		invalid)
			printf '%s\n' \
				'{"id":"22222222-2222-2222-2222-222222222222","status":"Invalid"}'
			;;
		transport-error)
			printf '%s\n' 'notary service unavailable' >&2
			exit 1
			;;
	esac
elif [[ $1 == notarytool && $2 == log ]]; then
	mkdir -p "$(dirname "$4")"
	printf '%s\n' \
		'{"status":"Invalid","issues":[{"message":"The signature is invalid."}]}' \
		> "$4"
else
	printf 'unexpected xcrun invocation: %s\n' "$*" >&2
	exit 1
fi
SCRIPT
chmod +x "$scratch/bin/xcrun"

PATH="$scratch/bin:$PATH" \
	FAKE_NOTARY_CALLS="$calls" \
	FAKE_NOTARY_MODE=accepted \
	"$notarize" "$artifact" test-profile "$log_output" >/dev/null
[[ ! -e $log_output ]]
[[ $(grep -c '^notarytool submit ' "$calls") -eq 1 ]]
if grep -q '^notarytool log ' "$calls"; then
	printf 'Accepted submissions must not request a rejection log\n' >&2
	exit 1
fi

: > "$calls"
if PATH="$scratch/bin:$PATH" \
	FAKE_NOTARY_CALLS="$calls" \
	FAKE_NOTARY_MODE=invalid \
	"$notarize" "$artifact" test-profile "$log_output" >/dev/null 2>&1; then
	printf 'Invalid notarization must fail\n' >&2
	exit 1
fi
grep -F '"The signature is invalid."' "$log_output" >/dev/null
grep -F \
	'notarytool log 22222222-2222-2222-2222-222222222222' \
	"$calls" >/dev/null

: > "$calls"
rm -f "$log_output"
if PATH="$scratch/bin:$PATH" \
	FAKE_NOTARY_CALLS="$calls" \
	FAKE_NOTARY_MODE=transport-error \
	"$notarize" "$artifact" test-profile "$log_output" >/dev/null 2>&1; then
	printf 'Notary transport errors must fail\n' >&2
	exit 1
fi
[[ ! -e $log_output ]]
if grep -q '^notarytool log ' "$calls"; then
	printf 'A submission without an ID cannot request a rejection log\n' >&2
	exit 1
fi

printf 'Notarization helper tests passed.\n'
