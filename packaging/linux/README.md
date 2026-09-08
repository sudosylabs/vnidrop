# Linux packaging

The Linux release workflow builds the Rust GTK 4/libadwaita app in `linux/`:

- `.deb` on Ubuntu 24.04, for Ubuntu 24.04 or newer
- `.rpm` on Fedora 43, for systems meeting that build's RPM dependencies

Packages contain the native executable and desktop integration files. GTK,
libadwaita, and other system libraries are package-manager dependencies. No JVM
is bundled or required. Downloads use SHA-256 checksums; an APT or RPM repository
with signed metadata is not configured.

## GitHub Actions

[Linux packages](../../.github/workflows/linux-packages.yml) runs for relevant
pull requests, manual dispatches, and calls from the coordinated release workflow.
Both jobs build GTK, test headless MIME verification, and validate the package.
The Debian job also runs the native Linux logic tests. GTK interaction tests run
in the separate [Native GNOME workflow](../../.github/workflows/linux-gnome.yml).

Manual and coordinated-release builds require the repository variable
`VNIDROP_DIAGNOSTICS_ENDPOINT` and secret `VNIDROP_DIAGNOSTICS_INGEST_KEY`.
The release caller passes them to the reusable workflow. Build and payload
verification both require embedded diagnostics configuration. Pull-request builds
explicitly disable reporting and receive neither value.

Artifacts retain the names expected by the release assembler:

| Package | Output directory | Workflow artifact |
|---|---|---|
| DEB | `build/release/linux/deb/` | `vnidrop-<version>-linux-deb-x64` |
| RPM | `build/release/linux/rpm/` | `vnidrop-<version>-linux-rpm-x64` |

Each directory contains its package and SHA-256 checksum. Manual and release runs
upload artifacts; pull requests only build and verify them. The coordinated
[release workflow](../release/README.md) publishes the GitHub Release.

## Install or upgrade

Verify the downloaded files against the release's `SHA256SUMS`:

```bash
sha256sum -c SHA256SUMS
```

Quit VniDrop before installing or upgrading. On Ubuntu 24.04 or newer:

```bash
sudo apt install ./vnidrop_VERSION-1_amd64.deb
```

On Fedora 43:

```bash
sudo dnf install ./vnidrop-VERSION-1.x86_64.rpm
```

The package name remains `vnidrop`, so the package manager replaces the previous
Compose package. The native app uses the existing `~/.vnidrop` profile and reads
Compose preferences on first launch when native preferences are absent. It writes
subsequent preferences to a separate JSON file. Package recipes leave the profile
and received files in place. See the [native app guide](../../linux/README.md)
for identity storage and profile handling.

The GTK DEB raises the minimum baseline from Ubuntu 22.04 to 24.04. Ubuntu 22.04
users need an OS upgrade before installing it; the package manager enforces the
new library dependencies. Uninstall with `sudo apt remove vnidrop` or
`sudo dnf remove vnidrop`.

## Build locally

Install Rust and the GTK/libadwaita development tools listed in the
[native app guide](../../linux/README.md). Build DEB on Ubuntu 24.04 or newer and
RPM on Fedora 43 with `rpm-build`. The workflow records the complete tool lists.

Configure diagnostics as described above, then run from the repository root:

```bash
make package-deb
make package-rpm
```

The existing `make package-linux-native-deb` and `make package-linux-native-rpm`
commands select the same builds. For an intentionally unconfigured development
package, pass `VNIDROP_DIAGNOSTICS_REQUIRED=0`. Official builds require reporting.

Legacy Compose packaging remains available as `make package-compose-deb` and
`make package-compose-rpm` for migration checks. It requires the old JDK/Gradle
setup and writes to `build/release/linux-compose/`, outside the release inputs.
