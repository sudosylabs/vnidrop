# Transfers

## Domain model

A **Transfer draft** is a local selection, editable name, and destination intent.
It is not durable. The core may perform ephemeral **Transfer preparation** to
import and hash its sources.

A transfer exists when it is durably registered, not when a receiver approves
it or bytes begin to move.

| Kind | Identity | Recipient model | Approval | History |
|---|---|---|---|---|
| Invitation transfer | Ticket and imported content | Anyone holding the wrapped ticket, subject to policy | Required when policy demands it | Durable after creation |
| Targeted transfer | Immutable sender, receiver, manifest, and content identity | Exactly one saved-device relationship | Explicit before content | Durable from registration, except successful pre-approval abandonment |

The canonical terms and their precise definitions live in
[`CONTEXT.md`](../../CONTEXT.md).

## Invitation transfers

**Current:** the sender imports content into the Iroh blob store and creates a
VniDrop invitation ticket. A receiver presents that ticket through the handshake
protocol. Access policy and approval gate content access. Accepted content is
fetched from Iroh and published through a no-overwrite receive sink. A receipt
completes the observable lifecycle.

The wrapped `vnd1:` ticket carries VniDrop metadata and backup relay addresses.
A raw Iroh `BlobTicket` is not an application invitation and is rejected.

See [Core transfer flow](../../crates/vnidrop/CORE_FLOW.md) for the detailed
protocol and output-sink rules.

## Targeted transfers: current interface

**Current:** the public targeted-creation call blocks through negotiation and
approval. Platforms have compensated for that shape by combining calls, event
payloads, and presentation state to discover the transfer ID and retain the
process. That splits lifecycle authority and leaves preparation/import outside
the durable status model.

The durable store remains authoritative, but the call shape makes concurrent
stop and caller recovery harder than necessary.

## Targeted transfers: selected target interface

**Target:** callers use an opaque preparation handle:

```text
newTargetedTransferPreparation(receiver) -> preparation

preparation.send(sources, name) -> TargetedTransfer
preparation.stop() -> StopOutcome

runtimeObligationFacts() -> neutral core facts
```

`send` returns after durable registration. Negotiation and approval continue
under core ownership independently of the initiating platform call. The
returned `TargetedTransfer` identity is stable and can be read through existing
get/list operations.

Preparation is core-owned but ephemeral. Stopping it before durable registration
leaves no transfer history.

### Lifecycle outcomes

- **Abandon:** stop intent commits before approval. The core revokes work and
  removes the newly registered transfer; it does not appear in history.
- **Cancel:** approval commits first, or later lifecycle work is active. The
  transfer becomes durable `Cancelled` history.
- **Fail:** a registered offer or later lifecycle operation fails. The failed
  transfer remains in history.
- **Retry:** creates a new Targeted transfer with a new identity; it does not
  mutate failed history into another attempt.

The core linearizes the approval/abandon race. If abandon wins, a late approval
is rejected. If approval wins, the same stop intent is a cancellation. Callers
receive a precise stop outcome and do not infer the winner from timing.

The existing blocking creation call remains a compatibility adapter over the new
module during migration.

## Runtime obligations

A **Runtime obligation** is a core-owned fact that VniDrop must remain available
for active byte work or sender content availability. It includes transfer
preparation, invitation sharing, targeted negotiation/provider availability,
and active invitation or targeted receiving.

Passive offers, terminal history, and retry housekeeping are not Runtime
obligations. The core exposes neutral facts; Android or Apple adapters decide how
those facts map to a foreground service, background task, or other platform
mechanism. Notifications consume lifecycle facts separately and do not own
retention policy.

## File data path

**Invariant:** Rust streams bytes. Platform adapters provide paths or open file
descriptors and keep the corresponding access lease alive.

- Desktop shares paths; Rust performs directory walking when requested.
- Android shares file descriptors for files. SAF directory trees are expanded
  in Kotlin into per-file descriptors and relative names; directory descriptors
  are never passed to Rust.
- Android receives to MediaStore Downloads by default or to a selected SAF tree.
- Apple keeps security-scoped access alive for as long as the core uses a URL.
- Receive publication uses temporary output plus a no-clobber link/rename policy.
