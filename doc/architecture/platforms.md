# Platforms

## Shared boundary

VniDrop has a shared Kotlin/Compose application for Android, Windows, and Linux,
plus a native SwiftUI application for Apple platforms. Both call the same Rust
core through generated UniFFI bindings.

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

Shared presentation follows MVVM-style ViewModels with state flows and named
methods. The application graph owns application-lifetime services and read
models. A composable must not become the authority for runtime retention merely
because it observes transfer UI state.

Android source adapters open file descriptors for files. Folder sharing walks a
SAF tree in Kotlin and supplies per-file descriptors and relative names. Desktop
adapters use paths and let Rust walk directories.

Android receive output defaults to MediaStore Downloads; custom destinations use
SAF sinks. The retention adapter must be idempotent and teardown-safe because
platform lifecycle callbacks can repeat or race.

## Swift and Apple platforms

SwiftUI owns Apple presentation and platform file access. Security-scoped leases
must remain active until Rust completes or aborts its use of a source or
destination. Apple read models use the same domain terms and scenario matrix as
Kotlin, without requiring identical presentation code.

## Saved Devices read models

**Target:** deepen one in-process Saved Devices read-model module per platform.
It combines durable relationship and transfer reads into stable UI facts,
including which actions are currently meaningful. Views consume those facts
instead of re-deriving lifecycle rules.

The two platform modules should share contract scenarios for:

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

**Current:** the preparation interface returns durable Targeted identity
directly, and neither platform parses event JSON for identity. The old payload
remains temporarily for compatibility and is retired only through an explicit
contract version.

Retention consumes neutral Runtime obligation facts from the core and is owned
at application-graph lifetime. Notifications are a separate module: they may
observe the same lifecycle, but notification visibility never decides whether
the process must remain alive.

The core emits a payload-free `runtime_obligation/changed` wake-up around
ephemeral preparation changes. Android and Apple respond by re-reading the fact
snapshot; the event itself never carries retention policy.

## Verification boundary

Most lifecycle tests target the deep Rust module interface with private fault
adapters for network, store, and timing failures. A smaller public UniFFI suite
protects the cross-language contract. Kotlin and Swift tests protect their read
models and platform mappings, using the same canonical lifecycle scenarios.
