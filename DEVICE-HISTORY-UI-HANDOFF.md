# Saved Devices UI handoff

This document coordinates the platform UI work built on the production Saved
Devices and Targeted Transfer core.

## Product contract

- Saved Device is a top-level product feature, not an experimental setting.
- A populated Saved Devices screen has a title-only header. Explanatory copy
  belongs in the first-use empty state or next to the control that needs it.
- The main screen lists saved devices and outstanding consent requests. It does
  not expose the global Targeted Transfer history.
- Selecting a saved device opens a platform-native details surface: bottom
  sheet on compact mobile layouts and a native inspector, sheet, or dialog on
  wider layouts. That surface owns Send, label/forget/block actions, and the
  device's Targeted Transfers with related lifecycle activity, status,
  progress, and available actions.
- Display `localLabel` when present, otherwise the authenticated
  `remoteDisplayName`. Keep the endpoint ID secondary and diagnostic.
- Use each platform's native device iconography and interaction conventions.
  Equivalent behavior may use separate Apple and Compose implementations.
- Label changes are transactional from the UI's perspective: preserve the
  draft and editor on failure, prevent conflicting dismissal/edit actions while
  saving, and close only after success.
- Invitation Transfer and Targeted Transfer source composition have file,
  folder, editable-name, replacement, and cleanup parity. Keep the domains
  distinct after creation.
- Every Targeted Transfer still needs receiver approval. Saving a device never
  grants automatic receipt.
- UI and platform code manage pickers and destinations; Rust streams payload
  bytes. Android folder selection expands SAF trees into file descriptors and
  relative names, never a directory descriptor.

Use the exact domain terms in [`CONTEXT.md`](CONTEXT.md) and the security and
lifecycle invariants in [`crates/vnidrop/CORE_FLOW.md`](crates/vnidrop/CORE_FLOW.md).
The KMP implementation under
`shared/src/commonMain/kotlin/com/vnidrop/app/feature/saveddevices/` is a tested
behavioral reference, not an Apple visual specification.

## KMP implementation

Follow [`shared/AGENTS.md`](shared/AGENTS.md) plus
[`.codex/skills/compose-skill/SKILL.md`](.codex/skills/compose-skill/SKILL.md).

The KMP implementation owns:

- `shared/`, `androidApp/`, and `desktopApp/` Saved Devices presentation work;
- Material Android and native-feeling Windows/Linux presentations;
- per-device details, transfer composition, offers, pairing consent, label
  editing, and lifecycle actions;
- common state-machine tests, JVM Compose interaction tests, and platform
  adapter tests.

Before handoff, run `make check-localization`, `make check-shared`, and the
relevant Android build. Inspect the actual Android emulator and desktop window;
record any host that could not be rendered.

## Apple implementation

Apple remains native SwiftUI; do not add Apple presentation to `shared/`.

The Apple implementation owns:

- a top-level Saved Devices destination in the iOS tab bar and macOS sidebar;
- an Apple-native Saved Devices model/coordinator and SwiftUI screen;
- pairing consent and Targeted Offer presentation outside Experimental
  Settings;
- per-device details and Targeted Transfer lifecycle actions;
- iOS/macOS picker and receive-destination integration using the existing
  platform services;
- Swift model tests, UI contract tests, and simulator-rendered visual checks.

Use the generated production UniFFI surface in
`apple/VnidropCore/Sources/VnidropCore/Vnidrop.swift`, including
`listSavedDevices`, `listDeviceRelationships`, `listPairingEligibilities`,
`listPendingTargetedOffers`, `createTargetedTransfer`,
`respondToTargetedOffer`, `listTargetedTransfers`, receive/resume/cancel/delete,
label, forget, and block operations. Wrap those calls through the existing
Apple `CoreGateway` / `CoreRepository` boundary rather than invoking generated
bindings from SwiftUI views.

If startup reports a missing or corrupted endpoint identity, platforms must
offer the explicit `resetUnrecoverableIdentityWithLimitsAndNetworkConfig`
recovery flow rather than an endless Retry action. Confirm that Saved devices
must be paired again; transfer history and received files are retained. Locked
or temporarily unavailable credential storage remains retryable and must not
offer identity replacement.

Use SF Symbols and native iOS/macOS controls even when that duplicates Compose
presentation code. Share behavior and vocabulary across platforms, not widget
implementations. Before handoff, run `make check-localization` and
`make check-apple`, then inspect the affected iOS and macOS states in real
simulator/app hosts.

## Completion gate

Each platform PR is ready only when it demonstrates:

1. mutual consent creates and names a Saved Device correctly;
2. an already-saved pair is not prompted to save again;
3. files and folders can be composed, changed, and sent to one saved device;
4. Targeted Transfers are absent from the main device list and visible in the
   selected device's details surface;
5. receive, resume, cancel, delete, progress, and terminal states survive
   refresh/restart as defined by the core snapshot;
6. label failure preserves the draft and retry path;
7. empty, populated, busy, error, long-name, and destructive-confirmation
   states are rendered and visually inspected;
8. ordinary Invitation Transfer flows remain unchanged.
