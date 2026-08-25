#!/usr/bin/env bash

set -euo pipefail

if (( $# != 3 )); then
	echo "Usage: $0 <deb|rpm> <MAJOR.MINOR.PATCH> <package-path>" >&2
	exit 2
fi

format=$1
version=$2
package_path=$3

fail() {
	echo "Package verification failed: $*" >&2
	exit 1
}

require_command() {
	command -v "$1" >/dev/null 2>&1 || fail "required command '$1' is not installed"
}

[[ -f $package_path ]] || fail "package not found: $package_path"
require_command find
require_command unzip

extract_root=$(mktemp -d)
trap 'rm -rf "$extract_root"' EXIT

case $format in
	deb)
		require_command dpkg-deb
		[[ $(dpkg-deb -f "$package_path" Package) == "vnidrop" ]] || fail "unexpected Debian package name"
		[[ $(dpkg-deb -f "$package_path" Version) == "$version-1" ]] || fail "unexpected Debian package version"
		[[ $(dpkg-deb -f "$package_path" Architecture) == "amd64" ]] || fail "unexpected Debian architecture"
		[[ $(dpkg-deb -f "$package_path" Maintainer) == *"support@sudosy.fr"* ]] || fail "unexpected Debian maintainer"
		deb_dependencies=$(dpkg-deb -f "$package_path" Depends)
		deb_libc_pattern='(^|,[[:space:]])libc6([[:space:](]|,|$)'
		deb_xdg_pattern='(^|,[[:space:]])xdg-utils([[:space:](]|,|$)'
		[[ $deb_dependencies =~ $deb_libc_pattern ]] || fail "Debian package does not declare libc6"
		[[ $deb_dependencies =~ $deb_xdg_pattern ]] || fail "Debian package does not declare xdg-utils"
		dpkg-deb -x "$package_path" "$extract_root"
		;;
	rpm)
		require_command rpm
		require_command rpm2cpio
		require_command cpio
		metadata=$(rpm -qp --queryformat '%{NAME}\n%{VERSION}\n%{RELEASE}\n%{ARCH}\n%{LICENSE}\n' "$package_path")
		mapfile -t fields <<< "$metadata"
		[[ ${fields[0]:-} == "vnidrop" ]] || fail "unexpected RPM package name"
		[[ ${fields[1]:-} == "$version" ]] || fail "unexpected RPM package version"
		[[ ${fields[2]:-} == "1" ]] || fail "unexpected RPM release"
		[[ ${fields[3]:-} == "x86_64" ]] || fail "unexpected RPM architecture"
		[[ ${fields[4]:-} == "Apache-2.0" ]] || fail "unexpected RPM license"
		mapfile -t runtime_requirements < <(rpm -qpR "$package_path" | grep -Ev '^(rpmlib\(|/bin/sh$)' || true)
		printf '%s\n' "${runtime_requirements[@]}" | grep -Eq '^(glibc($|[[:space:]])|libc\.so\.6)' || fail "RPM package does not declare glibc"
		printf '%s\n' "${runtime_requirements[@]}" | grep -Fxq 'xdg-utils' || fail "RPM package does not declare xdg-utils"
		rpm2cpio "$package_path" | (
			cd "$extract_root"
			cpio -idm --quiet
		)
		;;
	*)
		fail "unsupported package format: $format"
		;;
esac

mapfile -d '' -t launchers < <(find "$extract_root" -type f -iname 'vnidrop' -perm /111 -print0)
(( ${#launchers[@]} == 1 )) || fail "expected exactly one executable VniDrop launcher"

mapfile -d '' -t desktop_entries < <(find "$extract_root" -type f -name '*.desktop' -print0)
(( ${#desktop_entries[@]} == 1 )) || fail "expected exactly one desktop entry"
grep -Eiq '^Exec=.*/VniDrop([[:space:]]|$)' "${desktop_entries[0]}" || fail "desktop entry does not launch VniDrop"
grep -Eq '^MimeType=.*application/vnd\.vnidrop\.transfer(;|$)' "${desktop_entries[0]}" || fail "desktop entry does not register VniDrop invitations"
desktop_icon=$(sed -n 's/^Icon=//p' "${desktop_entries[0]}")
[[ $desktop_icon == /* ]] || fail "desktop entry does not use an absolute icon path"
[[ -f "$extract_root$desktop_icon" ]] || fail "desktop entry icon is missing from the package"
if [[ $format == deb ]]; then
	script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
	"$script_dir/ensure-desktop-identity.sh" --check "${desktop_entries[0]}" ||
		fail "desktop entry does not match the VniDrop window class"
fi

mapfile -d '' -t bundled_jvms < <(find "$extract_root" -type f -path '*/lib/runtime/lib/server/libjvm.so' -print0)
(( ${#bundled_jvms[@]} == 1 )) || fail "expected exactly one bundled JVM"
mapfile -d '' -t runtime_releases < <(find "$extract_root" -type f -path '*/lib/runtime/release' -print0)
(( ${#runtime_releases[@]} == 1 )) || fail "expected exactly one bundled JVM release descriptor"
grep -Eq '^MODULES="([^" ]+ )*jdk\.unsupported( [^" ]+)*"$' "${runtime_releases[0]}" ||
	fail "bundled JVM does not contain jdk.unsupported"

mapfile -d '' -t debug_rust_jars < <(find "$extract_root" -type f -name 'shared-linux-x86-64-debug-*.jar' -print0)
(( ${#debug_rust_jars[@]} == 0 )) || fail "package contains a debug Rust runtime JAR"

mapfile -d '' -t rust_jars < <(
	find "$extract_root" -type f -name 'shared-linux-x86-64-*.jar' ! -name 'shared-linux-x86-64-debug-*.jar' -print0
)
(( ${#rust_jars[@]} == 1 )) || fail "expected exactly one release Rust runtime JAR"

native_entry_size=$(unzip -p "${rust_jars[0]}" 'linux-x86-64/libvnidrop.so' | wc -c)
[[ $native_entry_size =~ ^[0-9]+$ ]] || fail "release Rust runtime JAR does not contain libvnidrop.so"
(( native_entry_size > 0 )) || fail "release Rust runtime JAR contains an empty libvnidrop.so"

mapfile -d '' -t shared_jars < <(find "$extract_root" -type f -name 'shared-jvm-*.jar' -print0)
(( ${#shared_jars[@]} == 1 )) || fail "expected exactly one shared JVM JAR"
unzip -p "${shared_jars[0]}" META-INF/MANIFEST.MF | tr -d '\r' |
	grep -Fxq "Implementation-Version: $version" || fail "packaged app version does not match $version"

mapfile -d '' -t desktop_jars < <(find "$extract_root" -type f -name 'desktopApp-*.jar' -print0)
(( ${#desktop_jars[@]} == 1 )) || fail "expected exactly one desktop application JAR"
unzip -Z1 "${desktop_jars[0]}" | grep -Fxq 'app-icon.png' || fail "desktop application JAR does not contain app-icon.png"

printf 'Verified %s package: %s\n' "$format" "$package_path"
