# Saved Devices read-model scenario matrix

This is the canonical behavior matrix for the Kotlin and Apple in-process Saved
Devices read models. Each platform implements the matrix independently over the
same Rust reads. It is a contract for presentation facts, not a request for a
cross-platform UI implementation or a revisioned Rust aggregate snapshot.

## Durable read inputs

The read model accepts the current results of:

- pairing eligibilities;
- device relationships;
- saved devices;
- pending Targeted offers;
- Targeted transfer history.

Events are not an input. An event only wakes the platform, which performs the
reads again. Reconstructing the read model after process restart with the same
durable inputs produces the same relationship, history, and action facts.

Pending Targeted offers are live-session state and can disappear on restart,
timeout, disconnect, or sender cancellation. The durable transfer history is
still derived independently of that queue.

## Relationship scenarios

| Core read result | Main Saved Devices fact | Available decision |
|---|---|---|
| Pairing eligibility only | Eligible peer, not a Saved device | Remember or decline |
| `PendingIncoming` relationship | Outstanding consent request | Accept or decline |
| `PendingOutgoing` relationship | Outstanding consent request | No local decision while the peer answers |
| Saved-device row | Saved device | Open details; send, label, forget, or block there |
| `Saved` relationship without a saved-device row | No usable device row | Refresh on the next signal; do not synthesize identity data |
| `Revoked` / forgotten relationship | No pending or saved row | No action on the absent relationship |
| `Blocked` relationship | No pending or saved row | Unblocking is not part of this feature surface |
| Identity reset | Old Saved-device rows are absent from current reads | Devices must pair again; retained transfer roles still determine direction |

An incoming request outranks an eligibility in the single prompt host because
the remote peer is waiting for the local decision. Dismissing an eligibility is
session-local suppression, not a durable decline; the eligibility remains in
the main list.

## Targeted transfer action matrix

The same action facts apply on Kotlin and Apple. `Receive` and `Resume` are
receiver-only because they pull bytes into a local destination.

| Durable state | Sender actions | Receiver actions | Receiver progress |
|---|---|---|---|
| `Preparing` | Cancel | Cancel | Hidden |
| `Offering` | Cancel | Cancel | Hidden |
| `AwaitingApproval` | Cancel | Cancel | Hidden |
| `Approved` | Cancel | Receive, Cancel | Hidden |
| `Connecting` | Cancel | Cancel | Visible when total size is known |
| `Transferring` | Cancel | Cancel | Visible when total size is known |
| `Interrupted` | Cancel | Resume, Cancel | Preserved and visible when total size is known |
| `Completed` | Delete | Delete | Hidden |
| `Declined` | Delete | Delete | Hidden |
| `Cancelled` | Delete | Delete | Hidden |
| `Failed` | Delete | Delete | Hidden |
| `Deleted` | Not rendered | Not rendered | Hidden |

Transfer direction comes from the durable sender/receiver role recorded by the
core, not by comparing endpoints with the installation's current identity. This
keeps history correct after identity reset.

## Lifecycle scenarios

| Scenario | Read-model result |
|---|---|
| Cancel completes | Durable `Cancelled` row with Delete action |
| Sender abandons before approval | No history row after cancellation and removal settle |
| Approval wins the stop race | Durable cancellation semantics; eventual `Cancelled` row |
| Offer or delivery fails after registration | Durable `Failed` row with Delete action |
| Retry after failure | A new transfer identity; both history rows remain independently derived |
| Event is missed | Next refresh derives entirely from current reads |
| Process restarts | Durable relationships and transfers reconstruct the same facts; live offers may be gone |

The current public core contract does not expose abandonment as a separate
durable state. Until the preparation interface lands, Apple temporarily hides
an abandoned transfer identity while its cancel-and-delete compatibility path
settles. Removing that overlay belongs to the platform migration wave, after the
core owns abandonment directly.

## Test mapping

- Kotlin: `SavedDevicesReadModelTest`
- Apple: `SavedDevicesReadModelTests`

Both suites cover sender and receiver actions, receiver-only progress, sorting
and joins, deleted-row filtering, role-based direction after identity reset,
terminal relationship filtering, cancel/race/failure/retry history inputs, and
event-independent rebuilds. A live offer without a receiver-side durable
transfer row remains separate from transfer history, characterizing the current
offer/history boundary without synthesizing lifecycle state.

## Wave 1 Runtime obligation matrix

Runtime retention is a separate application-lifetime policy; notification
visibility does not affect it.

| Core fact | Sender obligation | Receiver obligation |
|---|---|---|
| Invitation `Importing` | Yes | Not a valid receiver combination |
| Invitation `Sharing` | Yes | Not a valid receiver combination |
| Invitation `Receiving` | Not a valid sender combination | Yes |
| Targeted `Preparing` | Yes once represented by a durable row | No |
| Targeted `Offering` / `AwaitingApproval` | Yes | No |
| Targeted `Approved` / `Connecting` / `Transferring` | Yes | Yes |
| Targeted `Interrupted` | Yes while sender content remains available | No active receive obligation |
| Any terminal state | No | No |

The current contract cannot expose Targeted Transfer preparation before durable
registration. Wave 3 replaces this platform-derived matrix with neutral core
Runtime obligation facts and covers that missing interval.
