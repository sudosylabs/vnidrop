#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 3 ]]; then
	printf 'Usage: %s <artifact> <keychain-profile> <log-output>\n' "$0" >&2
	exit 2
fi

artifact=$1
keychain_profile=$2
log_output=$3

[[ -s $artifact ]] || {
	printf 'error: notarization artifact is missing or empty: %s\n' "$artifact" >&2
	exit 1
}
[[ -n $keychain_profile ]] || {
	printf 'error: notarization keychain profile is empty\n' >&2
	exit 1
}
[[ -n $log_output ]] || {
	printf 'error: notarization log output path is empty\n' >&2
	exit 1
}

rm -f "$log_output"
set +e
response="$(
	xcrun notarytool submit "$artifact" \
		--keychain-profile "$keychain_profile" \
		--wait \
		--output-format json
)"
submit_exit=$?
set -e
printf '%s\n' "$response"

submission_id="$(
	printf '%s\n' "$response" |
		jq -r '.id // empty' 2>/dev/null ||
		true
)"
status="$(
	printf '%s\n' "$response" |
		jq -r '.status // empty' 2>/dev/null ||
		true
)"

if [[ $submit_exit -eq 0 && $status == Accepted && -n $submission_id ]]; then
	printf 'Notarization accepted (submission %s)\n' "$submission_id"
	exit 0
fi

printf 'error: notarization was not accepted (status: %s, submission: %s)\n' \
	"${status:-unknown}" "${submission_id:-unknown}" >&2
if [[ -n $submission_id ]]; then
	mkdir -p "$(dirname "$log_output")"
	if xcrun notarytool log "$submission_id" "$log_output" \
		--keychain-profile "$keychain_profile"; then
		printf '%s\n' 'Apple notarization log:' >&2
		cat "$log_output" >&2
	else
		printf 'error: could not retrieve the Apple notarization log\n' >&2
	fi
fi
exit 1
