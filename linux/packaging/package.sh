#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
format=${1:?Usage: package.sh deb|rpm [native-binary] [output-directory]}
binary=${2:-$root/target/release/vnidrop-gnome}
output=${3:-$root/build/release/linux/$format}
version=$("$root/packaging/linux/resolve-version.sh")
[[ -f $binary && -x $binary ]] || { echo 'Build the native release binary first.' >&2; exit 1; }
[[ $format == deb || $format == rpm ]] || exit 2
[[ $(uname -m) == x86_64 ]] || { echo 'These package recipes currently target x86_64.' >&2; exit 1; }
file "$binary" | grep -q 'ELF 64-bit' || { echo 'Expected a native ELF binary.' >&2; exit 1; }
mkdir -p "$output"
output=$(cd "$output" && pwd)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
install -Dm755 "$binary" "$stage/usr/bin/vnidrop"
install -Dm644 "$root/LICENSE" "$stage/usr/share/licenses/vnidrop/LICENSE"
install -Dm644 "$root/linux/packaging/com.vnidrop.VniDrop.desktop" "$stage/usr/share/applications/com.vnidrop.VniDrop.desktop"
install -Dm644 "$root/linux/packaging/com.vnidrop.VniDrop.service" "$stage/usr/share/dbus-1/services/com.vnidrop.VniDrop.service"
install -Dm644 "$root/linux/packaging/com.vnidrop.VniDrop.xml" "$stage/usr/share/mime/packages/com.vnidrop.VniDrop.xml"
install -Dm644 "$root/assets/linux/app-icon.svg" "$stage/usr/share/icons/hicolor/scalable/apps/com.vnidrop.VniDrop.svg"
desktop-file-validate "$stage/usr/share/applications/com.vnidrop.VniDrop.desktop"
case $format in
  deb)
    mkdir -p "$stage/DEBIAN"
    cat > "$stage/DEBIAN/control" <<CONTROL
Package: vnidrop
Version: $version-1
Architecture: amd64
Maintainer: Sudosy Labs <support@sudosy.fr>
Section: net
Priority: optional
Depends: libc6 (>= 2.39), libgtk-4-1 (>= 4.10), libadwaita-1-0 (>= 1.5), libglib2.0-0t64 (>= 2.80), libgdk-pixbuf-2.0-0, libsecret-1-0, shared-mime-info, desktop-file-utils, xdg-utils
Description: Native GNOME peer-to-peer file transfer
 Send files and folders directly between devices with explicit approval.
CONTROL
    dpkg-deb --root-owner-group --build "$stage" "$output/vnidrop_${version}-1_amd64.deb"
    package="$output/vnidrop_${version}-1_amd64.deb"
    ;;
  rpm)
    rpmroot=$(mktemp -d)
    trap 'rm -rf "$stage" "$rpmroot"' EXIT
    mkdir -p "$rpmroot"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    tar -C "$stage" -czf "$rpmroot/SOURCES/payload.tar.gz" usr
    cat > "$rpmroot/SPECS/vnidrop.spec" <<SPEC
Name: vnidrop
Version: $version
Release: 1
Summary: Native GNOME peer-to-peer file transfer
License: Apache-2.0
URL: https://github.com/vnidrop/vnidrop
Source0: payload.tar.gz
Requires: gtk4 >= 4.10, libadwaita >= 1.5, libsecret, shared-mime-info, desktop-file-utils, xdg-utils
%global debug_package %{nil}
%description
Send files and folders directly between devices with explicit approval.
%prep
%build
%install
mkdir -p %{buildroot}
tar -xzf %{SOURCE0} -C %{buildroot}
%files
/usr/bin/vnidrop
/usr/share/licenses/vnidrop/LICENSE
/usr/share/applications/com.vnidrop.VniDrop.desktop
/usr/share/dbus-1/services/com.vnidrop.VniDrop.service
/usr/share/mime/packages/com.vnidrop.VniDrop.xml
/usr/share/icons/hicolor/scalable/apps/com.vnidrop.VniDrop.svg
SPEC
    rpmbuild --define "_topdir $rpmroot" -bb "$rpmroot/SPECS/vnidrop.spec"
    package="$output/vnidrop-${version}-1.x86_64.rpm"
    cp "$rpmroot/RPMS/x86_64/vnidrop-${version}-1.x86_64.rpm" "$package"
    ;;
esac
(cd "$output" && sha256sum "$(basename "$package")" > "$(basename "$package").sha256")
