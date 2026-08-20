# System overview

## Shape of the system

```text
Android / Windows / Linux UI          Apple SwiftUI
            |                              |
    platform file adapters        platform file adapters
            |                              |
       Kotlin/UniFFI facade  <---->  Swift/UniFFI facade
                         |
                     Rust core
        +----------------+----------------+
        |                |                |
  transfer modules   persistence       Iroh endpoint
  and policy         + secrets         + protocols
        |                                 |
        +---------- filesystem -----------+
```

The generated UniFFI boundary exposes coarse application operations and durable
read models. It must not become the owner of lifecycle rules or a transport for
file payloads.

## Ownership

| Layer | Owns | Does not own |
|---|---|---|
| Rust core | Transfer identity and lifecycle, approval and access policy, Iroh endpoint and protocols, SQLite state, recovery, payload streaming | Pickers, UI navigation, platform background-service APIs |
| Shared KMP | Android/desktop presentation, ViewModels, navigation, platform-neutral UI state | Transfer authority, duplicated durable lifecycle state |
| Apple app | SwiftUI presentation and Apple platform integration | Transfer authority, a separate transfer state machine |
| Platform adapters | Opening paths or descriptors, security-scoped/SAF leases, receive destinations, process-retention mechanisms | Protocol decisions or payload buffering as the default path |

## Module boundaries

The Rust runtime is split by responsibility under
[`crates/vnidrop/src/runtime/`](../../crates/vnidrop/src/runtime/): sharing,
receiving, lifecycle, provider behavior, the UniFFI facade, and startup wiring.
Persistence is assembled by `persistence::open_all`, while each domain store
owns its schema and operations.

**Current:** Targeted transfer behavior crosses runtime facade methods,
protocol handlers, callback seams, and platform workarounds. `CoreInner` must
coordinate too much of that behavior.

**Target:** one deep Targeted transfer module owns:

- immutable transfer identity and manifest;
- durable registration and lifecycle transitions;
- consent and authorization custody;
- concurrent stop, approval, and failure rules;
- cleanup and startup recovery;
- neutral Runtime obligation facts.

The Targeted wire protocol remains a narrow adapter. Device relationships remain
a separate deep module consulted through a private seam; they are not absorbed
into the transfer module.

## Change direction

The selected migration is additive:

1. Characterize current behavior and establish immutable identity.
2. Concentrate Targeted lifecycle rules behind the internal module interface.
3. Add the caller-optimized preparation/lifecycle interface and neutral Runtime
   obligation facts.
4. Migrate Kotlin and Swift read models and retention consumers in parallel.
5. Retain the old blocking creation call as a compatibility adapter until an
   explicit public contract version removes it.

This sequence preserves working public behavior while moving authority toward
the module that can enforce it atomically.
