#!/usr/bin/env bash

set -euo pipefail

if (( $# != 1 )); then
	echo "Usage: $0 <deb-package>" >&2
	exit 2
fi

package_path=$1
[[ -f $package_path ]] || {
	echo "Debian package not found: $package_path" >&2
	exit 1
}
command -v dpkg-deb >/dev/null 2>&1 || {
	echo "Required command 'dpkg-deb' is not installed" >&2
	exit 1
}
command -v md5sum >/dev/null 2>&1 || {
	echo "Required command 'md5sum' is not installed" >&2
	exit 1
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work_root=$(mktemp -d "${TMPDIR:-/tmp}/vnidrop-deb-patch.XXXXXX")
trap 'rm -rf "$work_root"' EXIT

extract_root="$work_root/package"
rebuilt_package="$work_root/rebuilt.deb"
dpkg-deb --raw-extract "$package_path" "$extract_root"

mapfile -d '' -t desktop_entries < <(find "$extract_root" -type f -name '*.desktop' -print0)
if (( ${#desktop_entries[@]} != 1 )); then
	echo "Expected exactly one desktop entry in Debian package" >&2
	exit 1
fi

desktop_entry=${desktop_entries[0]}
"$script_dir/ensure-desktop-identity.sh" "$desktop_entry"

md5sums="$extract_root/DEBIAN/md5sums"
if [[ -f $md5sums ]]; then
	relative_entry=${desktop_entry#"$extract_root/"}
	entry_checksum=$(md5sum "$desktop_entry" | awk '{ print $1 }')
	updated_md5sums="$work_root/md5sums"
	awk -v path="$relative_entry" -v replacement="$entry_checksum  $relative_entry" '
		{
			separator = index($0, "  ")
			if (separator > 0 && substr($0, separator + 2) == path) {
				if (!found) {
					print replacement
					found = 1
				}
				next
			}
			print
		}
		END {
			if (!found) {
				print replacement
			}
		}
	' "$md5sums" > "$updated_md5sums"
	cp "$updated_md5sums" "$md5sums"
fi

dpkg-deb --build --root-owner-group "$extract_root" "$rebuilt_package" >/dev/null
mv "$rebuilt_package" "$package_path"
