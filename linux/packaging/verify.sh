#!/usr/bin/env bash
set -euo pipefail
format=${1:?Usage: verify.sh deb|rpm package}
package=${2:?Package path required}
root=$(cd "$(dirname "$0")/../.." && pwd)
version=$("$root/packaging/linux/resolve-version.sh")
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
case $format in
  deb)
    [[ $(dpkg-deb -f "$package" Package) == vnidrop ]]
    [[ $(dpkg-deb -f "$package" Version) == "$version-1" ]]
    [[ $(dpkg-deb -f "$package" Architecture) == amd64 ]]
    dpkg-deb -f "$package" Depends | grep -q 'libadwaita-1-0 (>= 1.5)'
    dpkg-deb -x "$package" "$stage"
    ;;
  rpm)
    [[ $(rpm -qp --queryformat '%{NAME}-%{VERSION}-%{RELEASE}.%{ARCH}' "$package") == "vnidrop-$version-1.x86_64" ]]
    rpm -qpR "$package" | grep -q libadwaita
    rpm2cpio "$package" | (cd "$stage" && cpio -idm --quiet)
    ;;
  *) exit 2 ;;
esac
file "$stage/usr/bin/vnidrop" | grep -q 'ELF 64-bit'
case ${VNIDROP_DIAGNOSTICS_REQUIRED:-1} in
  1|true) "$stage/usr/bin/vnidrop" --check-diagnostics ;;
  0|false) ;;
  *) echo 'VNIDROP_DIAGNOSTICS_REQUIRED must be 0 or 1.' >&2; exit 2 ;;
esac
desktop="$stage/usr/share/applications/com.vnidrop.VniDrop.desktop"
desktop-file-validate "$desktop"
grep -Fxq 'Exec=vnidrop %F' "$desktop"
grep -Fxq 'DBusActivatable=true' "$desktop"
grep -Fxq 'StartupWMClass=com.vnidrop.VniDrop' "$desktop"
grep -Fxq 'MimeType=application/vnd.vnidrop.transfer;' "$desktop"
grep -Fxq 'Name=com.vnidrop.VniDrop' "$stage/usr/share/dbus-1/services/com.vnidrop.VniDrop.service"
[[ -s $stage/usr/share/icons/hicolor/scalable/apps/com.vnidrop.VniDrop.svg ]]
bash "$root/linux/packaging/verify-mime.sh" "$stage/usr/share"
[[ ! -e $stage/home && ! -e $stage/root ]]
echo 'Native package identity, binary, activation, icon, and MIME checks passed.'
