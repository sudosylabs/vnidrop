# Native GNOME application

`vnidrop-gnome` is a Rust frontend using GTK 4 and libadwaita, linked directly to
the existing VniDrop core. It uses Adwaita navigation, header bars, adaptive
dialogs, preferences, system file pickers, and system appearance. It contains
no Compose/JVM UI or transfer engine fork.

The native app includes invitation and targeted transfers, saved devices,
settings, previews, notifications, and bug reporting. The official Linux package
workflow now builds this frontend. The Compose host remains available for migration
checks.

The target is full feature and behavior parity with the Linux Compose app through
native GNOME presentation. See the [parity and native UX plan](PARITY_PLAN.md) for
the original audit, implementation evidence, and remaining release acceptance gates.

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
- An editable invitation draft with native file/folder pickers and drag and drop;
  add, replace, remove or clear sources; automatic transfer names, editable sender
  name and approval/public access mode. Cancelled pickers and failed preparation
  preserve the draft for retry.
- Asynchronous preparation, cancellation that retains the draft, QR/save
  invitation actions, and bounded UTF-8 invitation-file opening.
- Receive review with metadata and destination selection, sender approval/refusal,
  global pending-request notices, received-file opening and Show in Files, stop, and history removal.
- Native progress bars for known byte totals, waiting indicators for unknown totals,
  per-receiver delivery progress, and a timestamped activity dialog restored from
  persisted core events. Live byte updates preserve existing detail controls.
  Finished receivers move into Receiver history; pending approvals and ongoing
  receivers stay on the details page. Receiver history and Activity use bounded,
  scrollable dialogs. QR invitations keep their white background square.
- Devices: remember/decline after a completed transfer, explicit incoming consent,
  outgoing pending state, local labels, forget, block and unblock. Open from the
  device icon or application menu. Devices has a persistent list/detail page, with
  back navigation on narrow windows and a single full-page empty state. The header's
  Manage device action opens label, forget, block and unblock controls in a dialog.
  Review opens the pending decision and outlines its complete receiver/device card.
- Send files or a folder directly to a mutually saved device with a locked recipient.
  Incoming offers have explicit approval/refusal and a native destination picker;
  Review focuses and outlines the offer. Approved transfers can receive, interrupted
  transfers can resume, and active transfers can cancel. Finished transfers live in
  a bounded per-device history dialog with confirmed removal. History remains
  accessible after forgetting a device.
- Native preferences for appearance, name, receive folder/reset and notifications.
  Relay mode/URL editing validates input before a guarded network restart; failures
  restore the previous configuration or expose startup recovery.
- Storage usage includes cached transfer data, app data, received files still on
  disk, and stale partial files. Cache clearing restarts the core without removing
  received files, history or identity. Temporary cleanup removes VniDrop partials
  older than one day, preserving current partials and following no directory links.
- Native desktop notifications for approvals, incoming offers, pairing requests
  and transfer outcomes. Activation selects the associated transfer/device and
  focuses pending approval. Foreground suppression, deduplication and withdrawal
  respect the saved preference.
- Transfer speed/remaining-time estimates, bounded image previews in the existing
  `ui/previews/<transfer-id>.preview` cache, and plural-aware file counts. Invitation
  receive review supports an editable receiver name, folder access checks and retry.
- Native bug reports with required descriptions, optional contact and bounded recent
  activity summaries. Event payloads, file paths, invitations and peer identifiers
  are excluded from automatic report activity.
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
all blocking core calls run on worker threads. Event callbacks retain a bounded
history and wake a coalesced snapshot refresh. Cancel, approval, and shutdown can run while a
receive call is waiting. Native preference writes are serialized off the GTK
thread.

The mixed Transfers page is intentionally retained at the user's request; separate
Outgoing/Incoming navigation and additional history-management actions are excluded.
Closing the app stops sharing. Background-after-close and Flatpak remain optional
extensions, not requirements of the published KMP Linux baseline.

Local Linux testing was reported successful on 8 September 2026, and the GTK
frontend was selected for official packages. `make package-deb` and
`make package-rpm` build release binaries and validate package identity, diagnostics
configuration, and MIME integration. The longer `package-linux-native-*` names
remain aliases to the same builds. DEB requires Ubuntu 24.04+
(GTK 4.10+/libadwaita 1.5+); RPM builds use Fedora 43. Package recipes never edit
the user's profile. Release checks cover upgrades, cross-platform transfers,
and notification activation on the supported systems.

Bug reporting uses `VNIDROP_DIAGNOSTICS_ENDPOINT` and
`VNIDROP_DIAGNOSTICS_INGEST_KEY` at build time or runtime. When neither native
build variable is set, the native build reuses `vnidrop.diagnostics.endpoint` and
`vnidrop.diagnostics.ingestKey` from the repository's `gradle.properties`, overridden
by `$GRADLE_USER_HOME/gradle.properties` (default `~/.gradle/gradle.properties`).
The KMP `included` switch applies only to the KMP build. Native environment overrides
are treated as a pair, so an endpoint is never combined with another deployment's
saved key. Development builds can set both to empty to disable reporting. Values are embedded in the
binary, never printed by the build script. Configure both together,
using the existing diagnostics service base URL. HTTPS is required except for local
loopback test servers. Reports post to `/v1/bugs` and require an acknowledgement
matching the report ID; retries retain that ID for an unchanged draft. Unconfigured
builds explain that reporting is unavailable. No report is sent automatically.

The native package targets require reporting configuration by default
(`VNIDROP_DIAGNOSTICS_REQUIRED=1`). They reject missing, partial, non-HTTPS, or
loopback release configuration. The package verifier runs the extracted binary's
`--check-diagnostics` command, which checks embedded configuration without opening
GTK or displaying the endpoint/key; runtime overrides cannot satisfy this check.
For intentionally unconfigured development packages, explicitly set
`VNIDROP_DIAGNOSTICS_REQUIRED=0`.

The Linux package workflow supplies `vars.VNIDROP_DIAGNOSTICS_ENDPOINT` and
`secrets.VNIDROP_DIAGNOSTICS_INGEST_KEY` to manual and coordinated-release builds.
Pull-request builds explicitly disable reporting and receive neither value.
Packages and checksums go to `build/release/linux/deb/` or `build/release/linux/rpm/`.
The coordinated release workflow publishes those artifacts to GitHub Releases. The client ingest key is designed to ship in binaries and is not
an administrative or report-reading credential; see the diagnostics service's
[security model](../services/diagnostics-api/README.md#security-model).

The form retains failed drafts when closed/reopened during the same app session.
An unchanged retry reuses the complete original payload and report ID; changing the
report creates a new ID. Success is displayed only after a matching server receipt,
with a selectable reference. Connection, timeout, authentication, rate-limit,
server, and unconfirmed-delivery failures have distinct feedback. Optional activity
summaries omit event payloads; no report is sent without pressing Submit.


The UI uses genuine Adwaita widgets with small focus and layout styles. Automated Xvfb
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
bytes, checks narrow navigation and dark appearance, and closes the session. The
saved-device workflow also sends through the native composer, approves and receives
real bytes, cancels an offer, declines another, checks the core decline cooldown,
and removes a history entry. Settings tests exercise preference persistence, network
restart/identity preservation, cache clearing and the bounded report form. Preview
coverage restores the KMP cache after deleting the original image. Logic tests also
cover notification policy, plural rules, rate resets, stale-file cleanup and a local
HTTP diagnostics fixture. Logic coverage includes interrupted receive/resume,
stale consent, preparation cancellation, and retained history after forgetting.

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

Appearance changes apply and persist when selected; Save preferences applies the
name, receive destination, and notification settings. Preferences dropdowns wrap
long choices. About includes the product, privacy, version/platform, license, and
reporting information. Image thumbnails identify single-image invitation shares
(PNG/JPEG/WebP); they are shown in Transfers and its details, not folders,
multi-file shares, or direct device offers.
