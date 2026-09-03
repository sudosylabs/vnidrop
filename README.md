<p align="center">
  <img src="assets/1024x1024.png" alt="VniDrop app icon" width="128" />
</p>

<h1 align="center">VniDrop</h1>

<p align="center">
  <strong>Send files directly. Stay in control of who receives them.</strong>
</p>

<p align="center">
  Cross-platform file transfer for Android, iOS, macOS, Windows, and Linux.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Android-3DDC84?logo=android&logoColor=white" alt="Android" />
  <img src="https://img.shields.io/badge/iOS%20%26%20iPadOS-000000?logo=apple&logoColor=white" alt="iOS and iPadOS" />
  <img src="https://img.shields.io/badge/macOS-000000?logo=apple&logoColor=white" alt="macOS" />
  <img src="https://img.shields.io/badge/Windows-0078D4?logo=windows&logoColor=white" alt="Windows" />
  <img src="https://img.shields.io/badge/Linux-1793D1?logo=linux&logoColor=white" alt="Linux" />
</p>

<p align="center">
  <a href="https://github.com/vnidrop/vnidrop/actions/workflows/rust-core.yml"><img src="https://github.com/vnidrop/vnidrop/actions/workflows/rust-core.yml/badge.svg" alt="Rust core status" /></a>
  <a href="https://github.com/vnidrop/vnidrop/actions/workflows/shared-kmp.yml"><img src="https://github.com/vnidrop/vnidrop/actions/workflows/shared-kmp.yml/badge.svg" alt="Shared KMP status" /></a>
  <a href="https://github.com/vnidrop/vnidrop/actions/workflows/apple.yml"><img src="https://github.com/vnidrop/vnidrop/actions/workflows/apple.yml/badge.svg" alt="Apple status" /></a>
  <a href="https://github.com/vnidrop/vnidrop/actions/workflows/docs.yml"><img src="https://github.com/vnidrop/vnidrop/actions/workflows/docs.yml/badge.svg" alt="Docs website status" /></a>
</p>

<p align="center">
  <a href="https://github.com/vnidrop/vnidrop/actions/workflows/android-release.yml"><img src="https://github.com/vnidrop/vnidrop/actions/workflows/android-release.yml/badge.svg" alt="Android release package status" /></a>
  <a href="https://github.com/vnidrop/vnidrop/actions/workflows/apple-release.yml"><img src="https://github.com/vnidrop/vnidrop/actions/workflows/apple-release.yml/badge.svg" alt="Apple release status" /></a>
  <a href="https://github.com/vnidrop/vnidrop/actions/workflows/windows-store.yml"><img src="https://github.com/vnidrop/vnidrop/actions/workflows/windows-store.yml/badge.svg" alt="Windows Store package status" /></a>
  <a href="https://github.com/vnidrop/vnidrop/actions/workflows/linux-packages.yml"><img src="https://github.com/vnidrop/vnidrop/actions/workflows/linux-packages.yml/badge.svg" alt="Linux packages status" /></a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/status-early%20development-F59E0B" alt="Early development" />
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache%202.0-6D28D9" alt="Apache 2.0 license" /></a>
</p>

VniDrop moves files and folders from one device to another without first
uploading them to a file-hosting service. Choose what to send, decide who may
receive it, and share a small invitation. The receiving device uses that
invitation to find the sender and request the files.

The native C# / WinUI 3 Windows app is under development in [`windows/`](windows/README.md).
Its build instructions and migration status are separate from the current Compose
Windows release packages.

There is no account to create and no cloud copy of the transfer waiting after
you are done. The sender remains in control and can stop sharing at any time.

## How a transfer works

This is the invitation flow, used the first time two devices meet.

1. **Choose files or a folder.** VniDrop prepares the selection on the sender's
   device and keeps the original folder structure.
2. **Create an invitation.** The app produces a small VniDrop invitation that
   describes the transfer and how to reach the sender. Share it as a QR code, an
   NFC tag, or a `.vnd` file.
3. **Connect to the sender.** The receiver opens the invitation. Iroh helps the
   devices find each other and establishes an authenticated, end-to-end
   encrypted connection.
4. **Request access.** By default, the sender sees who wants the transfer and
   chooses whether to approve or refuse the request.
5. **Stream and verify.** After access is granted, `iroh-blobs` streams the files
   and verifies their content while it arrives. VniDrop saves each file directly
   to the chosen destination without replacing an existing file.
6. **Stay in control.** The sender can follow each receiver's progress, cancel a
   transfer, or stop sharing so the invitation can no longer be used.

Iroh tries to connect the devices directly, including across home routers and
mobile networks. If a direct path cannot be established, it can forward the
same end-to-end encrypted connection through a relay. The relay forwards
encrypted packets; it is not a VniDrop file store.

Once both sides have agreed to remember each other, steps 2 and 3 can be
avoided: the sender picks the device from its list and offers the transfer
directly, with no invitation to create or open. The devices still connect the
same way, and the receiver still approves. See
[Saved devices and targeted transfers](#saved-devices-and-targeted-transfers).

### Custom relay servers

VniDrop uses Iroh's public relay and discovery infrastructure by default. In
**Settings → Network**, users can select one of four policies:

- **Automatic (recommended):** use Iroh's public relays, with direct P2P/LAN
  connections whenever possible.
- **Strict custom:** use only up to eight configured custom HTTPS relays or
  direct connections. Startup reports an error if none of the custom relays can
  be established.
- **Custom with direct fallback:** prefer the configured custom relays, but
  continue with direct connections if they are unavailable.
- **Local only:** disable every relay and allow direct connections only,
  primarily for devices on the same network.

Strict custom, custom with direct fallback, and local only never use public
relays or public discovery, including relay addresses advertised by incoming
invitations.

Applying a relay change restarts VniDrop's network engine, so active transfers
and shares must be stopped first. The app tests the new configuration and
restores the previous one if it cannot connect. Invitations created for an old
relay configuration may need to be shared again; stopped shares never expose
their stale invitations. If a long relay profile makes an invitation too large
for a QR code, use the native share action or export the invitation file.

Relay credentials embedded in URLs are deliberately rejected and bearer-token
authentication is not currently supported. A self-hosted relay must either
accept the connecting endpoints or authorize their endpoint IDs independently;
the current device ID is shown in **Settings → Network** for this purpose.
Configure the same relay profile on participating devices. A custom relay needs
a TLS certificate issued by a publicly trusted WebPKI certificate authority;
private or enterprise CAs installed only in the operating system are not used
in this version. For resilient deployments, configure at least two relays in
different failure domains.

## Why Iroh and `iroh-blobs`?

VniDrop combines a networking layer with its own sharing rules:

| Layer | What it does |
|-------|--------------|
| [Iroh](https://docs.iroh.computer/) | Gives each device a secure identity, helps devices find one another, creates encrypted connections, and falls back to relays when a direct path is unavailable. |
| [`iroh-blobs`](https://docs.rs/iroh-blobs/0.103.0/iroh_blobs/) | Turns files into content-addressed, verified streams, so corrupted or unexpected data is detected while receiving. Multiple files are grouped into one collection. |
| **VniDrop** | Adds human-friendly invitations, receiver approval, per-transfer access rules, progress, history, cancellation, and safe saving on each operating system. |

Content addressing is useful here because the invitation identifies exactly
what was shared. The receiver does not simply trust a filename or claimed size:
the incoming content must match its expected hash.

## Approval is part of the transfer

A VniDrop invitation helps two devices meet, but the default invitation is not
automatic permission to download.

| Access mode | Behavior |
|-------------|----------|
| **Ask before each download** | The default. Every new receiver asks first, and the sender can approve or refuse the request. Approval gives that device temporary access to this transfer. |
| **Anyone with this transfer** | No interactive approval is required. Anyone holding the invitation may receive the files until the sender stops sharing. This mode is intended only for non-sensitive items. |

VniDrop starts from a deny-by-default position: it serves only the content in an
active share, and only when that receiver's access mode allows it. Unknown
content requests are rejected.

Treat an invitation like a private access link. Share it only with the intended
people, especially when using **Anyone with this transfer**.

## Saved devices and targeted transfers

After a completed transfer, both sides can agree to remember one another. A
remembered device is called a **saved device**, and sending to one is a
**targeted transfer**: no new invitation, QR code, or NFC tap is needed.

- **Mutual consent.** Saving requires a fully completed authenticated transfer
  plus an explicit confirmation on *both* devices. Consent is enforced
  cryptographically, not only in the interface.
- **Approval still applies.** Remembering a device never authorizes automatic
  receipt. Every targeted transfer waits for the receiver to approve it.
- **One sender, one receiver.** A targeted transfer is bound to the selected
  device identity, so a leaked ticket cannot authorize anyone else. Ordinary
  invitation-based sharing is unchanged and remains a separate mode.
- **Immediate local control.** Label, forget, block, revoke, cancel, and delete
  take effect on your device right away, even while the peer is offline.
- **Resumable.** An accepted transfer that was interrupted can continue once
  both devices are online again.
- **Still no server.** There is no relationship service, delivery queue, push
  service, inbox, or account. Both cores must be reachable while a transfer is
  negotiated, and identity keys and relationship secrets are stored in
  platform-backed credential storage.

A saved device identifies a VniDrop *installation*, not a person or a piece of
hardware. Reinstalling the app creates a new identity that must be saved again.

The security and lifecycle rules are documented in
[`crates/vnidrop/CORE_FLOW.md`](crates/vnidrop/CORE_FLOW.md).

## What VniDrop supports

- Individual files, multiple files, and complete folders
- QR codes, NFC tags, and portable `.vnd` invitation files
- Per-receiver requests, approvals, progress, and delivery status
- Cancel, stop sharing, and local transfer history
- Saved devices with mutual consent, and targeted transfers that need no new
  invitation
- Safe receive destinations that do not silently overwrite existing files
- Native SwiftUI apps on iOS, iPadOS, and macOS; Compose apps on Android,
  Windows, and Linux
- Strict custom HTTPS relay profiles with safe apply and rollback
- Optional user-submitted bug reports with transfer contents, invitations, and
  file paths excluded

## Privacy by design

- **No hosted transfer copy.** VniDrop does not upload file contents to a bug-report
  service or a VniDrop storage bucket.
- **Encrypted in transit.** Iroh connections are authenticated and encrypted
  end to end, including when a relay is needed.
- **Local control.** Transfer history and sharing state stay on the device.
- **Sensitive invitations.** An invitation can grant access, so it is
  deliberately excluded from product logs and bug reports.
- **Explicit access.** Approval is required by default, and stopping a share
  removes access immediately.

Please report suspected vulnerabilities through the private process in
[`SECURITY.md`](SECURITY.md), not through a public issue.

## Project status

VniDrop is in early development. The transfer engine, application experience,
and stored history format may change before a stable release. Build from source
if you want to try the current version.

```bash
git clone https://github.com/vnidrop/vnidrop.git
cd vnidrop

# List the supported development commands and check prerequisites
make help
make doctor

# Windows/Linux desktop
make run-desktop

# Android debug build
make build-android

# Build and launch the macOS app
make open-apple

# Open the native project for iOS, iPadOS, or Xcode development
make open-apple-project
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for prerequisites, development setup,
testing, and pull request guidance.

## Learn more

- [`doc/architecture/README.md`](doc/architecture/README.md) — architecture
  boundaries, subsystem guides, and selected target designs
- [`crates/vnidrop/CORE_FLOW.md`](crates/vnidrop/CORE_FLOW.md) — protocol,
  approval, durability, saved devices, targeted transfers, and file-handling
  details
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — development and contribution guide
- [`SECURITY.md`](SECURITY.md) — security policy and private reporting
- [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) — community standards

## License

VniDrop is available under the [Apache License 2.0](LICENSE).
