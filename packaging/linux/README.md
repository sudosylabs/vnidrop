# Linux packaging

VniDrop ships native x64 packages for the two common Linux package families:

- Debian/Ubuntu: `.deb`
- Current Fedora systems: `.rpm`

Both packages contain the application, its release Rust library, and a private
Java runtime. Users do not need to install Java separately. The packages are
currently distributed as direct GitHub Release downloads, so they use SHA-256
checksums rather than a Linux repository signing key. A future APT or RPM
repository should add repository metadata signing and its own update channel.

## GitHub Actions

The Linux packages workflow runs for relevant pull requests, release tags
matching `vMAJOR.MINOR.PATCH`, and manual dispatches. Each native package is
built and validated on its matching distribution family:

- `.deb` on Ubuntu 22.04 for a conservative glibc baseline
- `.rpm` inside Fedora 43 so `jpackage` can discover normal RPM dependencies

The shared JVM suite runs inside the Debian build job. Package construction and
payload validation happen in both build jobs, so there is no separate test
runner. Pull requests build and verify both packages but do not retain
artifacts. Manual runs retain build artifacts for 14 days. A pushed version tag
whose commit is on `master` creates the matching GitHub Release with the `.deb`,
`.rpm`, and a combined `SHA256SUMS`.

The legacy `v1.0.0` tag predates canonical versioning and does not define the
current product version. New release tags must match `version.properties`.

## Install a downloaded package

Verify downloads from the directory containing all three release files:

```bash
sha256sum -c SHA256SUMS
```

On Debian or Ubuntu:

```bash
sudo apt install ./vnidrop_VERSION-1_amd64.deb
```

On Fedora:

```bash
sudo dnf install ./vnidrop-VERSION-1.x86_64.rpm
```

The package manager installs declared system-library dependencies and creates
the VniDrop desktop launcher. Uninstall with `sudo apt remove vnidrop` or
`sudo dnf remove vnidrop`.

## Manual native builds

Use JDK 21 and Rust 1.91. Build DEB packages on Debian/Ubuntu with `dpkg` and
`fakeroot`; build RPM packages on Fedora with `rpm-build`. Building an RPM on
Ubuntu prevents `jpackage` from discovering normal RPM dependencies.

Set the release in `version.properties`. From the repository root on the
matching Linux family, run one of:

```bash
make package-deb
make package-rpm
```

The Make targets collect the Compose output under `build/release/linux/`, then
validate package identity, version, architecture, dependencies, bundled JVM,
and release Rust payload before generating a SHA-256 checksum.
