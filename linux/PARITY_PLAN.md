# Native Linux parity and UX plan

Status: approved for implementation; work started 7 September 2026. Source audit: repository commit `2ca10f1`, product version `0.3.3`, plus the local relay-startup fix. This document specifies the full port; the implementation evidence below records progress without closing incomplete parity areas.

## Product contract

Deliver the complete VniDrop Linux product through Rust, GTK and libadwaita. Preserve the KMP Linux app's capabilities, information, consent, state transitions, error recovery and persistence. Design navigation, controls, dialogs and system integration for GNOME. Native presentation is freedom to improve the interaction, not permission to remove functionality.

The current Linux implementation is an invitation-transfer slice. It is not the acceptance baseline for the finished port. Each capability below must have a usable native path and passing behavioral acceptance before replacing the published app. Intermediate implementation increments are allowed; they do not reduce release scope.

### Evidence and baseline

- KMP feature implementations, presentation models and tests define the behavioral reference. JVM adapters determine what is actually available on Linux.
- The audit uses the current checkout. Before release acceptance, pin and record the actual published Linux package, checksum and corresponding source revision. The current `version.properties` alone does not establish which code shipped. Reconcile any differences; preserve shipped behavior and include the current KMP features inventoried here.
- Apple and Windows are secondary references for native adaptation and shared-core integration. Their documented omissions are not automatic Linux scope exclusions. Some platform README statements are stale, so inspect implementation and tests before relying on them.
- This is a source-level audit, not a completed side-by-side visual or installed-package acceptance run. Existing Linux tests cover a subset of the product.

Reference areas:

| ID | Reference |
| --- | --- |
| R1 | [Transfer draft](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/send/TransferDraftViewModel.kt), [draft tests](../shared/src/commonTest/kotlin/com/vnidrop/app/feature/send/TransferDraftViewModelTest.kt) |
| R2 | [Send state](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/send/SendViewModel.kt), [details and activity](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/send/TransferDetails.kt), [preview storage](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/send/FilePreviewRepository.kt) |
| R3 | [Receive state](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/receive/ReceiveViewModel.kt), [external invitations](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/receive/ExternalInvitationController.kt) |
| R4 | [Approval coordinator](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/approvals/ApprovalCoordinator.kt), [approval tests](../shared/src/commonTest/kotlin/com/vnidrop/app/feature/approvals/ApprovalCoordinatorTest.kt) |
| R5 | [Saved devices](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/saveddevices/SavedDevicesViewModel.kt), [read model](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/saveddevices/SavedDevicesReadModel.kt), [targeted actions](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/saveddevices/SavedDeviceExperienceModels.kt) |
| R6 | [Settings state and operations](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/settings/SettingsViewModel.kt), [relay validation](../shared/src/commonMain/kotlin/com/vnidrop/app/feature/settings/RelaySettingsValidation.kt), [core adapter](../shared/src/commonMain/kotlin/com/vnidrop/app/core/CoreRepository.kt) |
| R7 | [Progress derivation](../shared/src/commonMain/kotlin/com/vnidrop/app/ui/state/AppUiModels.kt), [progress tests](../shared/src/commonTest/kotlin/com/vnidrop/app/ui/state/AppUiModelsTest.kt) |
| R8 | [Transfer notifications](../shared/src/commonMain/kotlin/com/vnidrop/app/notifications/TransferNotificationCoordinator.kt), [JVM notifications](../shared/src/jvmMain/kotlin/com/vnidrop/app/notifications/LocalNotificationService.jvm.kt), [bug reporting](../shared/src/commonMain/kotlin/com/vnidrop/app/diagnostics/BugReportService.kt) |
| R9 | [Desktop host](../desktopApp/src/main/kotlin/com/vnidrop/app/main.kt), [JVM receive methods](../shared/src/jvmMain/kotlin/com/vnidrop/app/feature/receive/ReceiveInvitationActions.jvm.kt), [JVM share methods](../shared/src/jvmMain/kotlin/com/vnidrop/app/feature/send/TransferShareActions.jvm.kt) |
| R10 | [Published packaging](../packaging/linux/README.md), [package workflow](../.github/workflows/linux-packages.yml), [native CI](../.github/workflows/linux-gnome.yml), [localization](../localization/README.md) |

## Feature and behavior inventory

“Partial” means some code exists, not that the flow has passed parity acceptance. “Missing” means the audited native host does not expose the capability. The acceptance column is required scope.

| ID / reference | Capability | Native today | Required native behavior and acceptance |
| --- | --- | --- | --- |
| P01 / R1 | Editable file/folder draft | Invitation and saved-device drafts implemented and tested | Open an empty draft; add/remove/clear/replace sources; multiple files or one folder; cancel a picker without losing existing input; handle duplicate and stale picker results as the reference does. Never delete original files. |
| P02 / R1 | Draft metadata | Invitation metadata and locked targeted recipient implemented | Preserve automatic-name provenance versus user edits; editable invitation sender name; explicit public-access warning; correct field validation. Targeted drafts lock the receiver and omit invitation-only sender/access controls. |
| P03 / R1 | Preparation, cancellation and retry | Invitation and targeted preparation/cancellation/retry implemented and tested | Show preparation state; single-flight submission; preserve valid draft after failure/cancellation; revalidate destination before targeted registration. Test cancellation both before registration and after its race with completion. |
| P04 / R2 | Outgoing catalog and details | Partial: combined send/receive list and basic facts | Keep the mixed Transfers view (user scope decision), with meaningful name, preview, count, size, status and active progress. Details retain metadata, files, sharing actions, receiver history and activity; all remain reachable at narrow widths. |
| P05 / R2 | Invitation distribution | QR display and `.vnd` save implemented; copy removed by user decision | Include scannable QR display and invitation-file export; only expose valid active invitations; cancellation of export is neutral. Verify an exported invitation opens on another host and the QR encodes the same invitation. |
| P06 / R2, R7 | Receiver delivery and transfer activity | Partial: bounded event history, byte/phase progress, receiver aggregation and activity dialog implemented; full progress acceptance remains | Per-receiver states and progress, distinct from sender availability; multi-file/multi-connection aggregation; actual transfer activity with useful timestamps. Preserve terminal outcomes and clear stale progress. |
| P07 / R2 | File previews | Native bounded PNG/JPEG/WebP thumbnails and KMP-compatible cache implemented; generic/type-icon fallbacks retained | Native file-type icons and bounded thumbnails where available; persistent outgoing preview cache compatible with existing `ui/previews`; restore after restart/source movement; fallback for old or unsupported files; cleanup and quotas. |
| P08 / R3, R9 | Invitation acquisition and review | Partial: file picker and command-line file open | Bounded, validated UTF-8 `.vnd` import; cold/warm activation; queued review; metadata and receiver-name editing; writable destination validation; explicit receive consent. Invalid input never starts a transfer. |
| P09 / R3, R7 | Receiving, recovery and completed files | Partial: blocking receive dispatched to worker, cancellation and artifact opening | Distinguish access request, connection, download, saving, completion, failure and cancellation. Show real progress and appropriate retry; preserve review input after recoverable failure. Open/reveal published files with missing/inaccessible-file feedback. |
| P10 / R3 | Received history management | Partial: individual transfer removal | Existing mixed history and individual removal retained; additional history-management changes excluded by user. removing history does not imply deleting received files. Verify filesystem and durable history effects separately. |
| P11 / R4 | Invitation approvals | Partial: banner and inline approve/refuse | App-wide pending queue with transfer/receiver context, per-request busy state, duplicate-response protection and remote withdrawal handling. Actionable while receive/preparation runs and while another view is selected. |
| P12 / R5 | Pairing eligibility and consent | Native consent and pending states in a Devices split page; broader desktop acceptance pending | Remember/decline an eligible device, incoming accept/decline, pending outgoing state, dismissal without fabricated consent, retry and stale-request handling. Both sides must consent through the existing core contract. |
| P13 / R5 | Saved-device management | Devices list/detail page, label/clear, forget, block/unblock implemented; full three-destination shell pending | Device list/details; local label, authenticated remote name, localized fallback; rename/clear label; forget/block with the reference consequences and confirmations; identity is secondary information, not the display name. |
| P14 / R1, R5 | Targeted sending | Native locked-recipient draft, core preparation/cancellation and durable status wired; native↔KMP acceptance pending | Compose for one saved device, lock destination, prepare and register through the core, show offering/awaiting approval and subsequent outcomes. Distinguish abandonment of a newly registered transfer from durable cancellation and from failure. |
| P15 / R5 | Targeted offers and receiving | Native offer review/accept/decline, receive/resume, progress, cancel and bounded per-device history wired; broader desktop acceptance pending | Pending-offer review with sender and contents; explicit accept/decline; receive approved offers; resume interrupted incoming transfers; cancellation/deletion only in allowed states; per-device history and verified-byte progress. |
| P16 / R6 | Preferences and appearance | Native name/theme/folder/reset/access check and notification preference; persistence regression tested | Preserve settings; folder selection, reset and access status; native theme behavior; errors that keep the displayed value consistent with persisted state. |
| P17 / R6 | Network configuration | Native four-mode editor, validation, restart and rollback wired; real Local only restart and injected failure recovery tested | Edit all four modes and custom URL list; reference validation; keep inactive custom URLs; explicit Apply, busy guard, controlled restart, restoration on failure and actionable restore failure. No automatic fallback to a more permissive network mode. |
| P18 / R6 | Storage management | Usage, guarded cache clearing and stale-part cleanup implemented; additional history deletion excluded by user | Usage breakdown, reclaimable space, cleanup, clear transfer cache, delete transfer records, busy guards and distinct destructive confirmations. Explain and test retained versus removed files; preserve the protected identity. |
| P19 / R8 | Notifications | Native GIO delivery and targeted activation wired; policy regression tested, real desktop-server qualification pending | Native notifications for the reference approval/transfer/device events, respecting preference, foreground state, deduplication and withdrawal. Clicking navigates to the relevant context; unsupported delivery has a truthful state. |
| P20 / R6, R8 | About and diagnostics | Native report form and bounded HTTPS transport with matching acknowledgement; local transport fixture tested, production configuration required | Product/version/device details; native bug-report form with required fields, optional contact/logs, bounded redaction, real configured transport, failure retry and success state. Saving a report alone does not replace submission parity. |
| P21 / R6, R9 | Startup and lifecycle | Partial: initialization, retry and close confirmation | Preserve existing profile/identity, preferences and histories; distinguish keyring, persistence and network failures with recovery actions. Serialise restart/shutdown against active work; prevent stale worker completions from changing a replacement session. |
| P22 / R10 | Localization and accessibility | Generated catalog, plural lookup, named controls and keyboard-friendly editors; Orca/high-contrast/scaling acceptance pending | All supported languages, plural rules, locale fallback, counts/dates/size formatting, keyboard reachability, named controls, focus restoration, screen-reader status, high contrast and scaling. Plural lookup supports the nine catalog languages. |
| P23 / R9, R10 | Installed desktop behavior | Native launcher/MIME/D-Bus activation and DEB/RPM recipes added; DEB payload smoke checks pass, installed upgrade acceptance pending | Desktop launcher, app icon/dock identity, `.vnd` MIME association, one instance per profile, notification activation, package dependencies and upgrades from published DEB/RPM. Do not call development command-line activation installed integration. |

### Explicit platform boundaries

| Capability | Evidence | Proposed treatment |
| --- | --- | --- |
| QR display | Shared outgoing share panel supports it | Required parity, P05. |
| Camera QR scanning / NFC read and write | JVM adapters hide these; unsupported implementations remain | Not part of the evidenced Linux baseline. Record as possible extensions; do not silently describe them as removed features. |
| System share sheet | JVM share adapter declares it unavailable | Native file export remains required. A system-sharing integration is an extension if a suitable Linux facility is chosen. |
| Paste invitation | No receive clipboard acquisition found in the audited feature/adapters | Proposed convenience extension, not a confirmed parity gap. Recheck pinned published build. |
| Continue after closing last window / autostart | Desktop host calls `exitApplication` on close; notification tray does not establish headless lifecycle | Preserve closing as exit, with the native active-work confirmation and orderly shutdown. Minimized/unfocused-window operation remains required. Background-after-close is a separate product decision. |
| Destructive identity reset | Available in the core and native Windows scope; no corresponding KMP UI path found in this audit | Actionable keyring recovery is required. Add reset only as an explicitly reviewed recovery flow with clear irreversible consequences. Never reset automatically to make startup succeed. |
| Flatpak | Current published channels are DEB/RPM | Optional distribution extension. It is not permission to replace a working DEB/RPM upgrade path. |

## Proposed native UX

### Main window

Use three persistent destinations: **Outgoing**, **Incoming**, and **Devices**. Preferences and About remain in the application menu. These labels are proposed native presentation of the existing Send, Receive and Saved devices capabilities, not new domain concepts.

Use a libadwaita view switcher for these three destinations, with the adaptive bottom switcher when the header no longer fits. Each destination retains selection and scroll position. Outgoing/Incoming can use a list-detail split at comfortable widths and push navigation at narrow widths. Devices has its own list/detail relationship. This avoids a permanent third column. GNOME documents this small-view-set and adaptive-switcher pattern in its [view-switcher guidance](https://developer.gnome.org/hig/patterns/nav/view-switchers.html).

```text
VniDrop                  Outgoing | Incoming | Devices            Menu
Pending requests: 2                                      Review

Outgoing list                         Selected invitation
New Transfer                         Name, preview, metadata
Name / state / size / progress       Share Invitation
...                                  Files | Receivers | Activity
                                     Stop Sharing / Remove…
```

This is information architecture, not a pixel mockup. Use Adwaita typography, symbolic system icons, ordinary lists, headers and spacing. Preserve VniDrop's identity through its app assets; do not reproduce Compose's decoration in GTK. List rows must retain useful status and progress rather than becoming large generic cards.

### Composition and receiving

- **New Transfer** opens a native action dialog containing editable selection, transfer name, sender name and invitation access policy. Pickers are actions within the draft. Its primary action creates the invitation and opens the sharing result. Large selections scroll within the content while controls stay reachable.
- **Send Files** on a device opens the same composition behavior with a locked receiver summary and the targeted primary action. No public-access option or invitation QR is shown for targeted creation.
- Picker cancellation preserves the draft; explicit dismissal discards only draft-owned temporary resources. Preparation has an explicit cancellation state, not an indefinitely disabled dialog. Failure leaves editable inputs and a retry path.
- **Open Invitation** and desktop activation enter the same receive review. Show sender, transfer name, count, size, receiver name, selected destination and its access status. Receiving begins only after confirmation. Completion navigates to the relevant Incoming record and offers Open/Show in Files.
- Use native action-dialog conventions and explicit verbs; use confirmations for irreversible operations rather than generic “OK”. This follows GNOME's [dialog guidance](https://developer.gnome.org/hig/patterns/feedback/dialogs.html).

### Transfer details

Provide complete details through local navigation or sections for Summary/Files, Receivers and Activity. Show QR Code opens a focused native dialog; Save Invitation exports the `.vnd` file. Copy Invitation is excluded by the user’s 7 September 2026 instruction. Keep stop/cancel/remove distinct and state-dependent. A stopped share must not retain usable sharing actions.

Show preparation and receive phases beside progress, and each receiver's progress beside that receiver. Do not turn “one receiver completed” into “sharing ended”. The transfer remains available according to core state. Empty receiver/activity sections should explain their own state without hiding that destination.

### Devices and consent

Devices presents attention items and saved devices separately: eligible devices, incoming pairing, outgoing pairing, and targeted offers must remain distinguishable. Device details show identity, local label, relationship actions, Send Files and targeted history.

New remote requests announce themselves through a persistent app-wide banner/badge and, when appropriate, a system notification. **Review** opens a native consent surface. This is a proposed GNOME adaptation of KMP's app-wide modal prompts: consent and discoverability are preserved without interrupting a user's current draft. Review must explicitly cover this adaptation.

Use one presentation coordinator for pending decisions, external invitation reviews and confirmations. Never stack modal dialogs or replace a draft with an incoming request. Pending items remain visible until resolved or withdrawn; revalidate before responding. A suggested review ordering is explicit user-requested review first, then outstanding remote requests by arrival time, with distinct labels for invitation approval, pairing and targeted offer.

### Preferences, feedback and integration

Use a native Preferences dialog with General, Appearance, Network, Notifications and Storage pages; expose Report a Problem and About from the app menu. These locations preserve all reference settings operations without mirroring the KMP settings overview.

Network changes use explicit Apply and preserve an editable draft during failures. Storage actions distinguish history, cached payload, temporary data and received originals. Bug reporting remains a real form and transport, not a link that silently drops fields or logs.

Use inline errors for correctable input, persistent startup feedback for unavailable services, and toasts for short operation results. A toast alone is insufficient when recovery requires changing input or retrying a failed operation. Cancellation is not an error notification.

Use GIO notification actions routed through the app to resolve current state before navigation. Notification integration includes matching installed desktop identity and D-Bus activation; it is not complete after calling `send_notification`. These requirements are documented in [GNotification](https://docs.gtk.org/gio/class.Notification.html). Custom development profiles need an explicit notification-activation strategy so a click cannot open the default profile accidentally.

## Implementation structure

Keep Rust as the transfer, authorization, identity and durable-state authority. The Linux frontend owns presentation models, user intent, native file access and system integration. No second transfer engine, parallel database, or Kotlin runtime belongs in the native app.

The current [session](src/session.rs) coalesces events into refresh notifications and drops their contents; its snapshot contains invitation transfers, receiver requests, artifacts and obligations only. That boundary must grow before progress, activity, pairing and targeted UI can be truthful.

| Module responsibility | Planned change |
| --- | --- |
| Session and event projection | Preserve bounded event/progress information and refresh durable state; include device relationships, eligibility, offers and targeted transfers; reconcile startup history. Identify asynchronous completions by session generation. |
| Lifecycle operations | Coordinate open, relay reconfiguration, cleanup and shutdown; reject conflicting operations, cancel synchronously where required, drain workers, restore prior config on restart failure. |
| Draft model | Session-scoped state with named actions, source IDs, name provenance, destination variants, single-flight preparation and cancellation; shared between invitation and targeted composition. |
| Presentation models | Typed invitation and targeted selection identities; separate action policies; progress aggregation and activity formatting; pending decisions. Test through meaningful behavior without GTK. |
| GTK feature modules | Split current `app/mod.rs`, `dialogs.rs` and `details.rs` by shell, outgoing, incoming, devices, drafts and settings as they are extended. Widgets bind state and invoke operations; callbacks do not become a second domain state machine. |
| Platform services | Native picker/open/reveal, preview cache, notification activation, preferences and diagnostics transport. Introduce an interface only where real platform/test substitution is useful. |
| Localization | Extend `localization/strings.json` targets as needed and regenerate; implement plural-aware native lookup. Never edit `linux/data/strings.json` directly. |

Use the existing public [core facade](../crates/vnidrop/src/runtime/facade.rs) for pairing, targeted transfers, history, storage usage and events. Audit lifecycle sequences in KMP/Windows before adding core APIs. Existing core capabilities do not by themselves establish frontend parity.

## Delivery sequence and acceptance gates

All increments belong to one full-parity release scope. Preserve the startup fix and existing working invitation flow while adding coverage. Each increment ends with real native UI verification and tests; none is a substitute for later increments.

| Increment | Work and dependencies | Gate |
| --- | --- | --- |
| 1. Baseline and release feasibility | Pin published package/source; reconcile P01–P23; decide compatible GTK/runtime packaging; capture reference behavior and native screen-state fixtures | Signed-off inventory of behavior and explicit extensions, plus a credible upgrade path for supported package users. No feature silently deferred out of release. |
| 2. Session and native shell | Event/read-model/lifecycle boundaries, typed selection, Outgoing/Incoming/Devices navigation, error/decision coordination | Old invitation tests still pass; simultaneous updates preserve focus/selection; inactive destinations still surface attention. |
| 3. Complete invitation workflows | P01–P11: editable drafts, receive review/retry, QR, previews, live progress, activity, approvals and history | Two real endpoints; verified bytes; multi-receiver progress; cancellation races; cold/warm invitation activation; long lists/names and narrow widths. |
| 4. Complete device workflows | P12–P15 using the same draft behavior and core APIs | Two-sided pairing, label/forget/block, targeted approval/decline/receive/resume, registration race and durable history; native↔KMP interoperability. |
| 5. Settings and lifecycle | P16–P18, P21; preserve migration; complete network restart and cleanup behavior | All four relay modes, restart rollback, active-work guard, folder failures and cleanup effects; protected identity survives restart and migration. |
| 6. Desktop integration and diagnostics | P19–P20, P22–P23; installed app identity, notifications, real report transport, localization and accessibility | Real desktop notification delivery/click; isolated report transport fixture; localized native screens; keyboard and Orca checks. Packaging integration begins in increment 1, not only here. |
| 7. Release qualification | Replace Compose package payload only after prior gates; keep package/upgrade identity | Installed DEB/RPM upgrade fixtures, restart and uninstall data preservation, cross-host transfer matrix, no unresolved required parity rows. |

## Validation plan

Use stable, synthetic profiles and files. Existing user data is for optional migration acceptance, never destructive fixtures. Translate reference tests into native behavior tests; do not merely recreate the same function calls with asserted constants.

| Scenario | Required evidence |
| --- | --- |
| Draft lifecycle | Add/remove/replace; automatic versus edited name; cancelled/stale picker; failure retry; cancellation on both sides of registration; original sources remain intact. |
| Invitations | Approval-required and public; two receivers; Unicode, empty files, folders and multi-file inputs; exported/QR invitation compatibility; stopped invitation unavailable; receive output byte equality. |
| Progress | Recorded core event sequences for interleaved receivers/connections, late events, interrupted operations, unknown totals and terminal transitions; activity remains distinct from current progress. |
| Receive failures | Invalid/oversized/non-UTF-8 invitation, destination unavailable, existing destination, storage failure, sender refusal/offline and user cancellation; assert UI recovery and durable outcomes. |
| Device consent | Eligible → request → consent on both endpoints; withdrawal while reviewing; simultaneous pending decisions; labels and block/forget consequences; no consent inferred from dismissal. |
| Targeted transfers | Approval and decline; interruption/resume; state-specific actions; abandoned registration versus durable cancellation; destination relationship changes during preparation. |
| Settings | Imported Automatic/LocalOnly with retained URLs, both custom modes, URL validation, busy guard, restart failure/restore, save failure, folder reset/access validation. |
| Storage and identity | Compare before/after records and files for each cleanup action; cache versus received originals; no key loss; unavailable/locked keyring without insecure fallback. |
| Notifications | Foreground/background preference rules, deduplication and withdrawal, click on stale request, running/cold app and correct profile. |
| Diagnostics | Required fields, opt-in log inclusion, redaction and byte bounds, installation ID preservation, real transport success/failure via local fixture; no real user report submitted during tests. |
| Native UX | Empty/loading/content/busy/error/confirmation states at narrow, medium and wide widths; long localized names; light/dark/high contrast; keyboard-only workflows and Orca. Preserve focus during live refresh. |
| Upgrade | Install pinned published package; create identity/preferences/history/received files; upgrade to native package; verify identity, relationships, retained URLs, files, launch/dock identity and `.vnd` routing; uninstall preserves user data. |

Existing executable gates from the repository root:

```sh
make check-linux
GSK_RENDERER=cairo xvfb-run -a make/with-secret-service.sh make test-linux-ui
make check-localization
```

Expand `test-linux-ui` beyond its current single invitation workflow. Run `make check-rust` if core implementation changes and the additional sink checks for sink/export/cancel changes. Package verification must be updated to validate the native payload; current checks explicitly expect a JVM and Compose output.

Maintain an evidence record for every P-row: source contract, native code, automated test, screenshots/manual acceptance where required, and open discrepancies. A row closes only after its complete user journey and failure recovery pass. Passing `make check-linux` alone cannot close the parity inventory.

## Decisions for review and unresolved release constraints

1. **Native UX proposal:** three destinations with contextual list/detail navigation; preferences in the app menu; attention-driven review rather than unsolicited modal stacking. These change presentation while preserving capabilities and explicit consent.
2. **Distribution compatibility:** published DEB packaging builds on Ubuntu 22.04, while current native code requests GTK 4.10+ and libadwaita 1.5+. Prove a compatible dependency/runtime/package strategy for existing supported users, or seek an explicit minimum-OS decision. Do not silently raise the requirement or substitute Flatpak for DEB/RPM. Fedora and non-GNOME session behavior also need installed acceptance.
3. **Published baseline pin:** release metadata, package checksums and source mapping are now recorded below. Downloading/verifying the actual package bytes and historical installed behavior remain release work.
4. **Extensions:** camera QR/NFC, clipboard acquisition, system sharing, headless operation/autostart, destructive identity reset and Flatpak are recorded separately above. None is needed to justify omitting an evidenced KMP Linux feature.

Reviewing this plan does not authorize publication or a destructive migration. Implementation should retain the full parity contract and report concrete remaining gaps until the release gates are satisfied.

## Implementation evidence

### Published baseline metadata

Queried the [v0.3.3 release](https://github.com/sudosylabs/vnidrop/releases/tag/v0.3.3) on 7 September 2026. The repository remote `vnidrop/vnidrop` redirects to `sudosylabs/vnidrop`. The downloaded `release-manifest.json` identifies source commit `9997ecd46393c01f3adf17305c68162056577296`, matching the local release tag. The manifest and published `SHA256SUMS` agree with the release API's Linux asset digests:

| Asset | Bytes | SHA-256 |
| --- | --- | --- |
| `vnidrop_0.3.3-1_amd64.deb` | 80472488 | `3324c0f08a89ef98e2bbca39678f176927726a43af77dccbf45867423ef84c74` |
| `vnidrop-0.3.3-1.x86_64.rpm` | 80497062 | `9fcf1e86b4d2f1894daf017b1141c2f0eb1c5155f3b094f196b2c93907a92e7b` |

Only the small manifest/checksum files were downloaded in this increment. These are expected digests for future package verification, not a claim that package installation or upgrade acceptance has passed. Linux assets were published on 28 August; some other assets on the same release were updated later.

### Invitation composition increment

- [Composition model](src/composer.rs): session-scoped source selection, stable source IDs, automatic versus edited naming, picker request IDs, validation, single-flight submission and retained retry input. [Behavior tests](src/composer_tests.rs) cover cancellation, failure, invalid folder/file combinations, duplicates, stale results and preservation of originals.
- [Native composer](src/app/composer.rs): editable file/folder selection, invitation metadata and access mode, preparation state, cancellation and retry. Incoming invitation activation queues behind the draft and resumes when it closes. Existing sender preparation cancellation/race tests remain in [session tests](src/session_tests.rs).
- [GTK workflow](src/app/tests.rs): open an empty draft via the app action; edit/add/remove/clear/replace; fail preparation after a selected file disappears; verify retained form fields; restore and retry; inspect the exported metadata; approve a real second endpoint; compare received bytes; drain the queued invitation. Native selection loading is exercised through the picker-result boundary; automated OS picker-dialog interaction is not claimed.
- Validation: `make check-linux` (15 logic/integration tests, formatting, Clippy and build), `make test-linux-ui`, and `make check-localization` pass. Real GTK screenshots inspected for empty, populated, narrow, dark and preparation-failure states. A dedicated Cairo screenshot renderer avoids repeated-capture artifacts from the window renderer.
- Reproduce captures with `VNIDROP_UI_SCREENSHOTS=/tmp/vnidrop-native-parity-ui make test-linux-ui` on an interactive Linux session. This increment does not complete the native shell, targeted composition, previews, QR, progress, devices, settings, notifications or package migration. Those remain required scope above.

### Icons, progress and activity increment

- [Native buttons](src/app/widgets.rs) use symbolic icons and accessible labels for file/folder selection and the welcome actions. Changing selection updates the label without discarding its icon; the GTK workflow checks that transition.
- [Event history](src/events.rs) merges persisted startup events with live callbacks, orders them across session revisions, deduplicates them and coalesces repeated byte updates within a 2,048-entry bound. Session tests verify completion callbacks and restoration after reopening a profile.
- [Progress model](src/progress.rs) handles preparation, receive/save phases, unknown totals, interleaved receivers and request IDs scoped to connections, interrupted streams and durable terminal states. Receiver matching understands the core's redacted endpoint fingerprints. Blob completion does not claim a delivered transfer.
- [Transfer details](src/app/details.rs) update native progress widgets without rebuilding controls for byte-only changes. [Activity](src/app/activity.rs) shows localized milestones and local timestamps, excluding raw payloads and byte-event noise. History remains bounded; this is not an unbounded audit-log viewer.
- The GTK workflow verifies actual approved delivery and byte equality, then uses an explicit progress fixture to verify fractions, retained widgets, and narrow/wide rendering. Activity screenshots use the real transfer events. Native screenshots were inspected for picker icons, receiver progress and completed activity.
- Checks: `make check-linux` (20 logic/integration tests, formatting, Clippy and build), `make test-linux-ui`, and `make check-localization`. Captures: `VNIDROP_UI_SCREENSHOTS=/tmp/vnidrop-native-progress-ui make test-linux-ui` in an interactive session.
- P06 remains partial: transfer rates/ETA, complete retry/resume and large multi-file progress acceptance still need work. Saved devices, targeted delivery, the three-destination shell and the remaining release gates retain their full scope.

### QR invitation and file actions increment

- User decision (7 September 2026): remove ticket copying; replace it with **Show QR code**. This supersedes the original copy-action entry in P05 without reducing invitation-file export.
- [Native QR dialog](src/app/qr.rs) generates the unchanged invitation off the GTK thread, renders black modules on a white quiet zone in both themes, supports narrow windows, and dismisses when the invitation becomes unavailable. Oversized invitations show a localized fallback directing the user to Save Invitation.
- [Encoder tests](src/qr.rs) use an independent decoder for UTF-8 and the reference capacity boundaries, including a version above 20. The GTK workflow decodes pixels from the actual wide/light and narrow/dark windows, then completes a real approved transfer using the decoded invitation. It also checks stopped-share dismissal and the oversized fallback.
- File and folder rows use native content-type icons. Received files have a **Show in Files** action through GTK's file launcher; launcher errors retain the existing actionable error feedback. File-manager/portal behavior across GNOME and other desktops still needs installed acceptance.
- Validation: `make check-linux` (21 logic/integration tests), `make test-linux-ui`, `make check-localization`; inspected QR and fallback screenshots in `/tmp/vnidrop-native-qr-ui`. Full saved-device/targeted workflows and the other open parity rows remain required.

### Saved-device consent and management increment

- [Device read model and commands](src/devices.rs) use the existing core APIs for eligibility, relationships, labels and deny rules. Commands re-read the current consent state and generation before applying a decision; stale dialogs cannot silently act on a replacement request. Labels take precedence over authenticated names; endpoint identity remains secondary.
- [Native device dialogs](src/app/devices.rs) expose eligible, incoming, outgoing, saved and blocked states. Closing consent leaves it undecided. Label input survives refresh; per-peer busy state prevents duplicate commands. Forget/block require explicit confirmation. Unblock explicitly does not restore saved access. Eligibility expiry schedules a snapshot refresh.
- The device icon/menu and attention banner make these flows reachable now. The approved three-destination shell remains pending; these dialogs do not close that navigation requirement or imply that targeted sending is implemented.
- [Session tests](src/session_tests.rs) use real endpoints and completed transfers to verify mutual consent, both decline paths, label normalization/clearing, stale decisions, forget, block and unblock. Concurrent snapshot tests exposed an event-lock lifetime across subsequent core reads; snapshot construction now releases event locks before any core operation.
- [GTK workflow](src/app/tests.rs) remembers a real peer, waits for remote consent, edits and clears its label, preserves unsaved input during refresh, and cancels a block confirmation without revoking access. Screenshots cover consent, pending pairing, narrow saved-device lists and destructive confirmation. Incoming consent rendering and multiple concurrent prompts still need broader desktop acceptance.
- Checks: `make check-linux` (24 logic/integration tests), `make test-linux-ui`, `make check-localization`. Captures: `VNIDROP_UI_SCREENSHOTS=/tmp/vnidrop-native-devices-ui make test-linux-ui` on an interactive desktop.

### Devices page and Review navigation correction

- User feedback supersedes the nested device dialogs: Devices is now a persistent native split page. The sidebar lists devices; the right pane shows consent, status and management. Narrow windows use list/back navigation. Selection and unfinished labels survive switching between Transfers and Devices. Only destructive confirmation uses a dialog.
- Global request banners remain visible on both pages. Transfer Review selects the pending transfer and reveals/focuses its approval action; device Review selects the incoming/eligible peer and reveals/focuses Allow/Remember. Review does not submit a decision.
- The regression test reproduced the original bug with `make test-linux-ui`: “Review must reveal the pending approval controls.” The prior handler selected a transfer but never moved into the decision area. [Navigation](src/app/navigation.rs) now waits for the page allocation before focusing the target control.
- [Navigation regression coverage](src/app/navigation_tests.rs) checks multiple pending requests, switching from Devices into transfer approval, incoming-device selection, visible action bounds and focus, narrow back navigation, wide layout, and refresh stability. The real-pairing workflow also verifies label preservation across page switches and confirms that cancelling Block leaves no nested dialog.
- Checks: `make check-linux`, `make test-linux-ui`, `make check-localization`. Rendered page and Review states captured under `/tmp/vnidrop-native-device-page-ui`. Outgoing/Incoming separation and direct-transfer workflows remain open scope; this increment addresses Devices and review routing.


### Device management and approval visual refinement

- User feedback moves label editing, forget, block and unblock into a dedicated management dialog opened from the device header. The page retains identity, state and consent decisions. Destructive confirmation replaces the management dialog, rather than stacking another device detail modal. Label drafts survive dismissal and reopening.
- An empty Devices page shows one centered status and a compact Transfers action across the window. Populated pages retain a device sidebar, detail pane and native back button at narrow widths. The panes stay in fixed GTK containers: the GTK workflow caught a GTK 4.14 accessibility assertion when NavigationSplitView reparented them after management closed. Resizing no longer reparents the panes.
- Compact icon-bearing request notices replace the stacked accent banners. Review outlines the specific receiver/device decision container and scrolls the whole receiver card into view, not just its approval button. Receiver cards have visible spacing; the main menu stays at the outer edge of the transfer header.
- The live approval regression first failed with “Review must ring the receiver container”. GTK coverage verifies the ring, full-card viewport bounds, spacing between receivers, full-width empty state, label dialog persistence, saved/cleared labels, cancellation without blocking, and resizing after closing management. Rendered states cover light and dark themes.
- Verification: `make check-linux`, `make test-linux-ui`, `make check-localization`. Optional captures: `VNIDROP_UI_SCREENSHOTS=/tmp/vnidrop-native-refined-devices make test-linux-ui`.


### Transfer detail density and dialog sizing

- QR rendering paints only the square code and its four-module quiet zone white. Surrounding drawing-area space stays transparent. The existing rendered-pixel decoder still verifies the unchanged invitation in wide/light and narrow/dark layouts; the dark-mode regression checks that the white surface does not fill the canvas.
- Finished receiver requests (completed, refused, expired, failed, cancelled or aborted) move into a Receiver history dialog opened from a counted row. Pending approvals, accepted receivers and unrecognized states remain visible on the details page. Durable core state controls this grouping; an interrupted stream alone does not finish a request. The history dialog updates as completions arrive and only includes the selected transfer.
- Activity and Receiver history use a fixed preferred height with internal scrolling, adapting to smaller windows. GTK regression coverage checks 40/41 receiver entries, 80 activity events, stable dialog height during live updates, and scrolling to the final row at wide and narrow sizes.
- Verification: `make check-linux`, `make test-linux-ui`, `make check-localization`. Rendered captures: `VNIDROP_UI_SCREENSHOTS=/tmp/vnidrop-transfer-details-refined make test-linux-ui`.


### Direct saved-device transfers and receiver spacing

- Receiver history now has a 12 px gap before active receiver cards. The rendered
  geometry regression first failed with “Receiver history needs spacing before the
  active receiver cards” and passes after the margin correction.
- Saved device details expose Send files using the native file/folder composer with
  a fixed recipient. The existing core preparation handle owns import, registration
  and cancellation; consent generation is checked again before submission. Device
  labels, block and forget remain in the management dialog.
- Incoming direct offers appear under their sender with explicit approve/refuse
  controls and destination selection. Global Review selects that device, scrolls to
  the offer, focuses approval and outlines the entire card. Receiving uses the core;
  interrupted transfers expose Resume, and active transfers retain Cancel while a
  receive call is running. Progress updates retain controls instead of rebuilding
  them for every byte update.
- Terminal direct transfers move into a counted Transfer history dialog with
  bounded scrolling and confirmed deletion. Historical peers stay visible after
  forgetting without acquiring send permission. No invitation ticket is created by
  this composer.
- The GTK workflow uses two real local endpoints to send and receive bytes, cancel
  a pending offer, decline another, respect the core's post-decline cooldown, and
  delete history through native controls. It also exercises wide/narrow layouts and
  offer focus. The history scenario caught standalone ActionRows without ListBox
  parents; cards now use valid GTK row containers, including keyboard traversal.
- Logic coverage verifies role/state actions, stale generation rejection,
  cancellation around preparation, failed destination recovery through Resume,
  successful bytes, decline, history deletion and retained history after forget.
  Native↔KMP interoperability and packaged desktop acceptance remain release gates.
- Verification: `make check-linux`, `make test-linux-ui`, `make check-localization`.
  Captures: `VNIDROP_UI_SCREENSHOTS=/tmp/vnidrop-native-direct-ui make test-linux-ui`.


### Remaining feature implementation, preserving mixed Transfers

The user explicitly retained the mixed Transfers page and excluded the proposed
Navigation/history redesign. This overrides the three-destination proposal above.
No additional clear-history action is added by this increment.

- Settings now include native general preferences, receive-folder reset and a real
  writable-folder probe, all four relay modes, retained inactive URL values,
  validation and guarded Apply. Reconfiguration drains in-flight reads, refuses
  active work and restores the previous policy on failure. A failed restoration
  exposes the startup Retry view. The GTK regression caught and fixed storage reads
  being mistaken for active transfer work.
- Storage reads core usage and currently present received-file sizes. Clear cache
  uses the core's shutdown-only API and reopens the same identity/profile. Free up
  space removes recognized partial files older than one day without following
  symlinks; delivered files, recent partials and history remain. This cleanup does
  not recursively empty unrelated trash directories.
- Notifications use GIO and one typed activation action. Pending notices are
  withdrawn when resolved, foreground/disabled delivery is suppressed, repeated
  snapshots do not duplicate notices, and historical completions are not replayed
  on startup. Review opens the matching transfer/device approval controls. Real
  desktop notification-server and installed cold-activation acceptance remain.
- Native progress controls show sampled speed and remaining time, resetting on
  phase changes, byte rewinds, completion and interruption. Preview generation is
  limited by input size, decoded dimensions and output size; the cache uses the
  existing KMP filenames and a 20 MiB quota. A regression restores previews after
  removing the original image and prunes orphan/oversized entries.
- Receive review edits the receiver name, verifies folder access before starting,
  retains retry input in the current session and exposes retry for failed/cancelled
  invitations. Plural-aware file counts use the source localization catalog.
- Bug reports preserve unchanged-draft request IDs across retry and use the existing
  `/v1/bugs` schema, required descriptions, optional contact and bounded event
  summaries without sensitive event payloads. Local HTTP fixtures verify receipt
  and rejection of mismatched acknowledgements; production endpoint/key are not
  embedded in the repository or contacted by tests.
- Native DEB/RPM packaging supplies a desktop entry, icon, `.vnd` MIME registration
  and D-Bus service. Separate build/validation targets and package CI leave the
  released KMP pipeline intact. DEB payload/MIME checks were exercised locally with
  the development binary; release/RPM builds and installed upgrade acceptance are
  separate gates. The GTK baseline requires Ubuntu 24.04+ for the current DEB recipe.

Checks for this increment: `make check-linux`, `make test-linux-ui`,
`make check-localization`, `cd localization && bun test`,
`bash -n linux/packaging/{package,verify}.sh`, and the native DEB package verifier.
Visual captures: `VNIDROP_UI_SCREENSHOTS=/tmp/vnidrop-port-settings-ui make test-linux-ui`.

### Preferences, About, and preview follow-up

- Theme selection applies immediately and persists independently of the general
  preferences Save button; the GTK regression closes/reopens Preferences to check
  both the displayed selection and GTK color scheme.
- Dropdowns use wrapping subtitles and wrapping popup labels. Narrow-width coverage
  checks every network option and verifies the longest choice is not ellipsized.
- Four storage strings were missing their Linux target; the source catalog now
  includes them. A catalog regression checks native references against generated
  English fallback entries.
- About now includes the KMP product/privacy explanation, native version/platform
  information, canonical privacy link, license, and a working Report a bug action.
- Native builds can reuse endpoint/key values from Gradle project/user properties,
  with explicit native environment overrides. The build does not log credentials;
  tests use a local fixture server and do not submit to the configured service.
- Single-image invitation shares are exercised through the composer and details.
  Thumbnails are applied after rebuilding list rows, fixing the generic-icon
  replacement. The same regression verifies both the list and detail thumbnail.

Verification: `make check-linux`, `xvfb-run -a make test-linux-ui`,
`make check-localization`, `cd localization && bun test`.
Captures: `VNIDROP_UI_SCREENSHOTS=/tmp/vnidrop-settings-fixes-ui` with the GTK suite.
