# Platforms

## Shared boundary

All frontends use the same Rust transfer core:

| Frontend | Source | Core access |
|---|---|---|
| Android Compose app | `androidApp/` and `shared/` | Generated Kotlin/UniFFI bindings |
| Windows WinUI app | `windows/` | Generated C#/UniFFI bindings |
| Apple SwiftUI app | `apple/` | Generated Swift/UniFFI bindings |
| Native Linux GTK/libadwaita app | `linux/` | Direct Rust calls |

The shared Kotlin module retains a JVM target for host-side tests. Desktop
applications use the native frontends listed above.

The platform/core boundary is capability-oriented: the platform obtains access
to a file or destination; Rust owns the transfer and streams bytes through that
access.

| Concern | Rust core | Platform/application layer |
|---|---|---|
| Transfer identity and lifecycle | Owns | Presents |
| Approval and access policy | Owns and persists | Collects user intent |
| Network and Iroh protocols | Owns | Configures through supported options |
| Source access | Consumes paths/descriptors | Picker, descriptor, and access lease |
| Receive destination | Streams and enforces sink contract | MediaStore, SAF, security-scoped URL, or desktop path adapter |
| Durable history | Owns snapshots | Builds platform read model |
| Process retention | Exposes neutral Runtime obligation facts | Maps facts to platform APIs |
| Notifications | Exposes lifecycle facts/events | Chooses platform notification behavior |

## Kotlin and Compose

Android presentation follows MVVM-style ViewModels with state flows and named
methods. The application graph owns application-lifetime services and read models.
A composable must not become the authority for Runtime obligations merely because
it observes transfer UI state.

Android source adapters open file descriptors for files. Folder sharing walks a
SAF tree in Kotlin and supplies per-file descriptors and relative names.
The JVM test adapters use paths and let Rust walk directories.

Android receive output defaults to MediaStore Downloads; custom destinations use
SAF sinks. The retention adapter must be idempotent and teardown-safe because
platform lifecycle callbacks can repeat or race.

## Swift and Apple platforms

SwiftUI owns Apple presentation and platform file access. Security-scoped leases
must remain active until Rust completes or aborts its use of a source or
destination. Apple read models use the same domain terms and scenario matrix as
Kotlin, without requiring identical presentation code.

## Saved Devices read models

Windows and Linux have separate native presentation implementations. The
Kotlin/Swift contract below records their shared scenario reference, not a
requirement to use KMP for desktop builds.

Kotlin and Swift each implement an in-process Saved Devices read model.
Each combines durable relationship and transfer reads into stable UI facts,
including which actions are currently meaningful. Views consume those facts
instead of re-deriving lifecycle rules.

The two platform modules share contract scenarios for:

- pending, accepted, blocked, forgotten, and reset relationships;
- active, cancelled, abandoned, failed, and retried Targeted transfers;
- stop/approval race outcomes;
- process restart and missed-event recovery;
- action availability for every durable state.

The canonical cases and expected facts are recorded in the
[Saved Devices read-model scenario matrix](saved-devices-scenario-matrix.md).

A Rust aggregate snapshot with revisions is intentionally deferred until tests
or production evidence demonstrate an unsolved torn-read problem.

## Events, retention, and notifications

Events wake platform observers, which then refresh authoritative reads.

**Current (domain contract v2):** the preparation interface returns durable
Targeted identity directly, and neither Kotlin nor Swift parses event JSON for identity.
Targeted events are payload-independent refresh hints.

Retention consumes neutral Runtime obligation facts from the core and is owned
at application-graph lifetime. Notifications are a separate module: they may
observe the same lifecycle, but whether a notification is currently displayed
never decides whether the process must remain alive.

Android additionally keeps the Iroh endpoint in a `dataSync` foreground service
while the user has opted into background notifications **and** at least one
saved device exists. Incoming targeted offers are not a core receiver obligation
until approved, but they cannot be delivered if the cached process is frozen.

The core emits a payload-free `runtime_obligation/changed` wake-up around
ephemeral preparation changes. Android and Apple respond by re-reading the fact
snapshot; the event itself never carries retention policy.

## Verification boundary

Most lifecycle tests target the deep Rust module interface with private fault
adapters for network, store, and timing failures. A smaller public UniFFI suite
protects the cross-language contract. Kotlin and Swift tests protect their read
models and platform mappings, using the same canonical lifecycle scenarios.

## Native Windows and Linux

WinUI owns Windows navigation, activation, file pickers, notifications, and
preferences. C#/UniFFI bindings call the same Rust core; managed code does not
stream transfer payloads. Build and run with
`pwsh windows/scripts/build.ps1 -Test -Run`. See the
[Windows guide](../../windows/README.md) for .NET and Windows SDK prerequisites.

The Linux app uses GTK 4/libadwaita and calls Rust directly. It owns native file
access, desktop integration, notifications, and Secret Service-backed
credentials. `make run-linux` starts it; `make package-deb` and
`make package-rpm` package it. See the [Linux guide](../../linux/README.md).

Native desktop builds do not use Gradle or bundle a JVM. The shared Kotlin JVM
target remains available through `make test-shared`.
