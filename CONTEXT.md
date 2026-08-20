# VniDrop

Local peer-to-peer file transfer. This glossary is the product/core ubiquitous language — not an implementation guide.

## Transfers

**Transfer draft**:
A temporary, local selection of files or one folder, an editable transfer name, and a destination intent before a transfer is durably registered. A draft becomes an Invitation transfer or a Targeted transfer at durable registration; later delivery or approval is lifecycle, not creation.
_Avoid_: pending transfer, temporary transfer, share draft

**Transfer preparation**:
Core-owned, ephemeral import and hashing of a submitted Transfer draft before durable registration. Abandoning preparation creates no transfer history.
_Avoid_: Targeted transfer, pending transfer, transfer attempt

**Invitation transfer**:
A share anyone with the ticket can request, subject to approval and access policy. Ordinary multi-recipient send/receive.
_Avoid_: contact send, held offer, reusable share offer

**Targeted transfer**:
A transfer bound to one saved-device relationship from durable registration onward: immutable sender, receiver, manifest, and content identity; requires explicit approval before content.
_Avoid_: contact transfer, private share

**Targeted transfer cancellation**:
Stopping a Targeted transfer while retaining it as durable `Cancelled` history.
_Avoid_: abandon, discard, delete

**Targeted transfer abandonment**:
Stopping and removing a newly registered Targeted transfer when the sender's intent commits before approval; it does not remain in history. If approval commits first, the same intent is Targeted transfer cancellation instead.
_Avoid_: cancel, failed transfer

**Failed targeted transfer**:
A durably registered Targeted transfer whose offer or later lifecycle failed. It remains in history; retrying creates a new Targeted transfer.
_Avoid_: abandoned transfer, transfer draft

**Runtime obligation**:
A core-owned fact that VniDrop must remain available for active byte work or sender content availability. Passive offers, terminal history, and retry housekeeping are not Runtime obligations.
_Avoid_: retention flag, active transfer, background task

**Saved device**:
A remote app identity this installation has mutually consented to remember, with directional grants at a relationship generation.
_Avoid_: contact, person, account

**Device relationship**:
The durable pairing state between this installation and a remote endpoint (pending, saved, forgotten/blocked lifecycle).
_Avoid_: contact record, friendship

## Persistence (core)

**Domain store**:
The module that owns schema and queries for one domain (invitation history, targeted transfers, blocked devices, relationship rows, pairing eligibility, secret metadata). Callers use store methods — never a raw SQL pool.
_Avoid_: repository-for-everything, DAO, database layer

**Invitation repository**:
The domain store for invitation-transfer history, artifacts, receiver requests, and related events. Module path `invitation`; today’s type name may still be `Repository`.
_Avoid_: “the database”, AppDataStores

**AppDataStores**:
The bag of concrete domain stores opened together for one app-data profile (one SQLite pool, every schema applied once).
_Avoid_: Repository (for the bag), Persistence (as a type name), DbContext

**Persistence open**:
Creating the profile’s SQLite pool, applying all domain schemas, and returning `AppDataStores`. The only place that may touch pool creation for app data.
_Avoid_: Repository::open as the global DB entry (once migrated), sqlite_pool export
