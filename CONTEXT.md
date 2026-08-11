# VniDrop

Local peer-to-peer file transfer. This glossary is the product/core ubiquitous language — not an implementation guide.

## Transfers

**Invitation transfer**:
A share anyone with the ticket can request, subject to approval and access policy. Ordinary multi-recipient send/receive.
_Avoid_: contact send, held offer, reusable share offer

**Targeted transfer**:
A transfer bound to one saved-device relationship: immutable sender, receiver, manifest, and content identity; requires explicit approval before content.
_Avoid_: contact transfer, private share

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
