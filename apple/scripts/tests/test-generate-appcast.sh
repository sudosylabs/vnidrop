#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/vnidrop-appcast-test.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/apple/scripts" "$scratch/apple/dist" "$scratch/packaging/version" "$scratch/bin"
cp "$script_dir/../generate-appcast.sh" "$scratch/apple/scripts/"
cat > "$scratch/packaging/version/resolve-version.sh" <<'SCRIPT'
#!/usr/bin/env bash
case "$1" in
  product) echo 0.3.5 ;;
  verify) exit 0 ;;
esac
SCRIPT
cat > "$scratch/bin/generate_appcast" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail
output=""
previous=""
directory=""
for argument in "$@"; do
  if [[ $previous == -o ]]; then output=$argument; fi
  previous=$argument
  directory=$argument
done
image=""
count=0
for path in "$directory"/*.dmg; do
  [[ -e $path ]] || continue
  image=$(basename "$path")
  count=$((count + 1))
done
[[ $count -eq 1 ]]
printf '<enclosure>%s</enclosure>\n' "$image" > "$output"
printf '%s\n' "$*" >> "$FAKE_CALLS"
SCRIPT
chmod +x "$scratch/packaging/version/resolve-version.sh" "$scratch/bin/generate_appcast"
printf 'arm\n' > "$scratch/apple/dist/VniDrop-0.3.5.dmg"
printf 'intel\n' > "$scratch/apple/dist/VniDrop-0.3.5-x86_64.dmg"

PATH="$scratch/bin:$PATH" \
	SPARKLE_BIN="$scratch/bin" \
	FAKE_CALLS="$scratch/calls" \
	bash "$scratch/apple/scripts/generate-appcast.sh" >"$scratch/output"

[[ "$(cat "$scratch/apple/dist/appcast.xml")" == "<enclosure>VniDrop-0.3.5.dmg</enclosure>" ]]
[[ "$(cat "$scratch/apple/dist/appcast-x86_64.xml")" == "<enclosure>VniDrop-0.3.5-x86_64.dmg</enclosure>" ]]
[[ "$(grep -c . "$scratch/calls")" -eq 2 ]]

rm "$scratch/apple/dist/VniDrop-0.3.5-x86_64.dmg"
if PATH="$scratch/bin:$PATH" SPARKLE_BIN="$scratch/bin" FAKE_CALLS="$scratch/calls" \
	bash "$scratch/apple/scripts/generate-appcast.sh" >"$scratch/output" 2>&1; then
	echo 'An Intel disk image is required for its update feed' >&2
	exit 1
fi
printf 'Appcast architecture tests passed.\n'
