#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/vnidrop-dmg-test.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/apple/scripts" "$scratch/packaging/version" "$scratch/bin"
cp "$script_dir/../build-dmg.sh" "$scratch/apple/scripts/"
printf 'PRODUCT_BUNDLE_IDENTIFIER: com.vnidrop.app\n' > "$scratch/apple/project.yml"

for tool in apple/scripts/build-core.sh apple/scripts/generate-appconfig.sh packaging/version/generate-apple-xcconfig.sh bin/xcodegen; do
	printf '#!/usr/bin/env bash\nexit 0\n' > "$scratch/$tool"
	chmod +x "$scratch/$tool"
done
cat > "$scratch/packaging/version/resolve-version.sh" <<'SCRIPT'
#!/usr/bin/env bash
case "$1" in
  product) echo 0.3.3 ;;
  apple-direct-build) echo 20260908.1200.00 ;;
  verify) exit 0 ;;
esac
SCRIPT
cat > "$scratch/bin/xcodebuild" <<'SCRIPT'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$FAKE_XCODE_CALLS"
if [[ " $* " == *" archive "* ]]; then
  while [[ $# -gt 0 ]]; do
    if [[ $1 == -archivePath ]]; then mkdir -p "$2"; break; fi
    shift
  done
  echo 'fixture archive failure' >&2
  exit 65
fi
exit 0
SCRIPT
# A successful formatter must never hide xcodebuild's failure.
printf '#!/usr/bin/env bash\ncat\n' > "$scratch/bin/xcbeautify"
chmod +x "$scratch/packaging/version/resolve-version.sh" "$scratch/bin/"*

if PATH="$scratch/bin:$PATH" \
	DEVELOPER_ID_APP='Developer ID Application: Example (ABCDEFGHIJ)' \
	DEVELOPMENT_TEAM=ABCDEFGHIJ \
	FAKE_XCODE_CALLS="$scratch/calls" \
	bash "$scratch/apple/scripts/build-dmg.sh" > "$scratch/output" 2>&1; then
	echo 'A failed archive must stop DMG packaging' >&2
	exit 1
fi
grep -F 'fixture archive failure' "$scratch/output" >/dev/null
if grep -F -- '-exportArchive' "$scratch/calls" >/dev/null; then
	echo 'A partially created archive must never be exported after xcodebuild fails' >&2
	exit 1
fi
printf 'DMG archive failure tests passed.\n'
