# Native GNOME application

`vnidrop-gnome` is a Rust frontend using GTK 4 and libadwaita, linked directly to
the existing VniDrop core. It uses Adwaita navigation, header bars, adaptive
dialogs, preferences, system file pickers, and system appearance. It contains
no Compose/JVM UI or transfer engine fork.

This is the first working invitation-transfer slice. The existing Compose
Linux host remains the release application while native feature coverage grows.

## Run on Linux

Use GTK **4.10+**, libadwaita **1.5+**, and Rust stable (verified with 1.91).
Ubuntu 24.04 provides suitable development libraries:

```sh
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev librsvg2-common
make run-linux
```

A working session D-Bus and unlocked Secret Service (normally GNOME Keyring)
are required for the protected device identity. The application displays a
recoverable startup error if that service is unavailable; it does not fall back
to storing private keys in a file.

The default profile is the existing `~/.vnidrop`. Quit the Compose host before
opening that profile in the native host. To experiment independently:

```sh
make run-linux LINUX_ARGS='--profile /tmp/vnidrop-gnome-trial'
```

`--profile` must name an absolute directory. Each custom profile has a distinct
application instance and device identity. Opening `.vnd` arguments routes them
to that profile's existing window and queues an explicit receive review:

```sh
target/debug/vnidrop-gnome --profile /tmp/vnidrop-gnome-trial invitation.vnd
```

## Included

- Empty state and transfer history, with a split view that becomes back/forward
  navigation on narrow windows.
- Multiple files or one folder, selected with the native picker or drag and drop;
  editable invitation name and approval/public access mode.
- Asynchronous preparation, cancellation that retains the draft, copy/save
  invitation actions, and bounded UTF-8 invitation-file opening.
- Receive review with metadata and destination selection, sender approval/refusal,
  a global pending-request banner, received-file opening, stop, and history removal.
- Native light/dark/system appearance, name and receive-folder preferences.
- Explicit confirmation before quitting with active work, followed by core shutdown
  and draining worker calls.

Existing SQLite history and Secret Service identity remain in place. When no
native preferences exist, the host reads the old DataStore preferences, including
relay mode/URLs, appearance, name, and destination. It saves subsequent changes to
`linux-preferences.json`, leaving the old preferences file untouched. Invalid
preferences fail startup rather than silently enabling a different network policy.
Do not relocate profiles without a separate protected-identity migration.

## Boundaries and remaining coverage

The core owns transfer state and bytes. GTK sends paths and receives metadata;
all blocking core calls run on worker threads. Event callbacks only wake a
coalesced snapshot refresh. Cancel, approval, and shutdown can run while a
receive call is waiting. Native preference writes are serialized off the GTK
thread.

Remaining before replacing the released Linux app: saved-device pairing and
targeted delivery, QR and clipboard invitation input, byte/rate progress,
notifications and background operation, editable relay settings, diagnostics,
and production desktop/MIME/Flatpak packaging. Relay policy is preserved and
shown read-only in this slice. Closing the app stops sharing.

The UI uses genuine Adwaita widgets without a custom CSS theme. Automated Xvfb
coverage and visual checks exercise GTK; a full GNOME Wayland session, desktop
portals, fractional scaling, and Orca still need acceptance testing. Flatpak will
also require deliberate Secret Service and profile migration work; the existing
Secret Service item store is not automatically replaced by the Secret portal.

## Development checks

```sh
make check-linux
# On a desktop with an unlocked keyring:
make test-linux-ui
# Headless, using an ephemeral keyring and disposable test profiles:
sudo apt install dbus-x11 gnome-keyring libsecret-tools xvfb xauth at-spi2-core
GSK_RENDERER=cairo xvfb-run -a make/with-secret-service.sh make test-linux-ui
```

The logic suite exercises migration, input bounds, live core transfers, approval
during blocking receive, cancellation before/after preparation, stale invitation
removal, and shutdown. The GTK test creates an invitation through the native
composer, approves a receiver through the native controls, compares received
bytes, checks narrow navigation and dark appearance, and closes the session.

`linux/Containerfile` supplies an Ubuntu development environment for hosts that
cannot run GTK Linux applications directly:

```sh
docker build -t vnidrop-gnome-dev -f linux/Containerfile .
docker run --rm -v "$PWD:/workspace" -v vnidrop-gnome-target:/workspace/target \
  vnidrop-gnome-dev bash -c 'rustup component add rustfmt clippy && make check-linux && GSK_RENDERER=cairo xvfb-run -a make/with-secret-service.sh make test-linux-ui'
```

The GUI is an explicit Cargo feature so ordinary Rust core builds and tests on
other platforms do not acquire GTK system dependencies. Localization is generated
from `localization/strings.json`; see the [localization guide](../localization/README.md).
