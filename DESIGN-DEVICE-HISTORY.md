# Design — Saved devices and targeted transfers

Status: **experimental foundation for the 0.3.x line**.

The unreleased contact/held-offer/polling prototype has been removed. The
implementation on this branch is the versioned saved-device, device-relationship,
and targeted-transfer foundation described below. Its wire protocol is
experimental and versioned; product UI remains deferred.

The feature lets two VniDrop installations remember one another after a
successful transfer, with explicit consent on both devices. A saved device can
then request a new transfer without another invitation, QR scan, or NFC tap.
The receiver must still approve every transfer.

The Rust core, protocol, persistence, credential-storage integration, and
platform contracts are the first delivery scope. Product UI is intentionally
deferred to a separate design and implementation session.

---

## 1. Vocabulary and invariants

### Saved device

A `SavedDevice` is a remote VniDrop **app-installation identity**. It is not a
person, account, address-book contact, or reliably identifiable piece of
physical hardware.

The identity is the remote iroh endpoint identity. A reinstall or unrecoverable
endpoint-key loss creates a new identity and requires a new successful transfer
and mutual consent. Display names, platform hints, IP addresses, and physical
device properties must never merge identities.

### Device relationship

A `DeviceRelationship` is a mutually acknowledged relationship between two
saved-device identities. It contains two directional grants: one issued in
each direction. The relationship is usable only after both grants have been
acknowledged.

### Targeted transfer

A `TargetedTransfer` is an immutable one-sender, one-receiver transfer. It is a
separate domain from the existing invitation-based `Share`, which may serve
multiple receivers.

The following invariants are mandatory:

- Saving a device requires a fully completed authenticated transfer and
  explicit consent on both devices.
- Remembering a device never authorizes automatic receipt. Every targeted
  transfer requires explicit receiver approval.
- A targeted transfer has exactly one sender identity, one receiver identity,
  one transfer ID, and one immutable manifest.
- Authorization is bound to the selected receiver. A leaked capability or
  ticket must not authorize any other identity.
- Relays may forward end-to-end encrypted traffic according to the active
  network profile, but VniDrop has no intermediary file store, relationship
  service, delivery queue, push service, or account system.
- Existing invitation-based transfers retain their current behavior and domain
  model.

---

## 2. Goals and non-goals

### Goals

- Send to a previously saved device without exchanging another invitation.
- Make mutual consent cryptographically enforceable rather than a UI promise.
- Keep receiver approval mandatory for each new transfer.
- Give forget, revoke, block, cancellation, and deletion immediate local
  security effect even when the peer is offline.
- Persist accepted interrupted transfers so they can resume when both devices
  are online again.
- Protect endpoint identity keys and relationship secrets with platform-backed
  credential storage.
- Provide versioned, typed Rust and UniFFI contracts that every platform can
  exercise before UI work begins.

### Non-goals

- Automatic acceptance or unattended writes to a receiver's device.
- Offline store-and-forward, automatic peer polling, background inboxes, or
  push notifications.
- Server-side device discovery, relationship storage, history synchronization,
  backup, export, or restoration onto another installation.
- Presence indicators or a promise that a suspended mobile application is
  reachable.
- Groups or a multi-recipient variant of `TargetedTransfer`.
- Associating several saved devices with a person or account.
- UI screens, navigation, wording, and presentation architecture in this phase.

---

## 3. Network and privacy model

Saved-device operations use the same configured iroh network profile as
ordinary transfers:

- `Automatic` may use configured/default relays and direct paths.
- Custom-relay modes remain restricted to their configured relays and fallback
  policy.
- `LocalOnly` must not silently enable public discovery or a relay.

An endpoint ID authenticates a peer; it is not, by itself, a routable address.
Address discovery and file transport may use a relay. VniDrop and the endpoints
still provide end-to-end authentication and encryption, so the relay cannot
decrypt content or authorize a recipient. A relay may observe transport
metadata such as network addresses, timing, and volume. VniDrop must not claim
that relayed traffic is anonymous, metadata-free, or relay-free.

VniDrop does not upload a transfer for later delivery. The sender and receiver
cores must both be reachable while an offer is negotiated. A relay cannot wake
a terminated or suspended application. The first release therefore reports a
typed unavailable or timeout result when the receiver's core cannot answer.

Current direct address candidates may be exchanged over an authenticated
connection and cached for the connection or a short local lifetime. The app
must not accumulate a historical IP-address log.

---

## 4. Identity and credential custody

The endpoint private key and all relationship capability secrets are protected
by platform-backed credential storage:

| Platform | Required protection |
|---|---|
| Apple | Keychain with a non-synchronizing, device-appropriate accessibility class |
| Android | Keystore-backed encryption; only ciphertext may live outside Keystore |
| Windows | DPAPI scoped to the current user |
| Linux | Secret Service/libsecret |

There is no plaintext fallback.

Rust owns identity use, cryptographic operations, relationship state, and
authorization. Platforms provide a narrow secure-secret-store adapter. Public
bindings exchange opaque handles and typed outcomes, never raw grants, pairing
tokens, or private keys.

If the endpoint identity key is temporarily unavailable, networking is
temporarily unavailable because VniDrop cannot authenticate as the same
endpoint. If the endpoint key is available but relationship grants are not,
ordinary invitation transfers remain available while saved-device operations
fail closed. Neither case may generate a replacement identity automatically.

### 4.1 Legacy endpoint-key migration

Migration of an existing endpoint key must be recoverable:

1. Read the legacy key.
2. Write it to protected storage.
3. Read it back and prove that it derives the same endpoint ID.
4. Commit a storage-version marker.
5. Only then remove the legacy copy.

A crash at any step must preserve at least one valid copy and must not change
the endpoint identity. Confirmed unrecoverable loss or an explicit identity
reset is required before replacement.

Secrets must not synchronize through platform cloud backup. Restored metadata
without its device-bound secrets reconciles to disabled relationships, never a
cloned identity.

---

## 5. Pairing eligibility

Only a **fully completed authenticated transfer** creates pairing eligibility.
A handshake, partial download, failed export, cancellation, decline, or failed
transfer does not qualify. Either the sender or receiver may initiate pairing
after a qualifying transfer.

During the qualifying transfer, the peers establish a cryptographic,
single-use pairing eligibility capability bound to:

- Both endpoint identities.
- The qualifying transfer/session.
- The saved-device protocol version.
- A 24-hour local expiry.

The capability becomes usable only after the transfer reaches its durable
completed state. It is stored locally in encrypted form without filenames or a
transfer-history record. It is deleted when consumed, declined, expired,
forgotten, blocked, or reset.

Requests without valid eligibility are silently rejected. This prevents a
modified stranger from generating unsolicited pairing prompts.

---

## 6. Mutual-consent protocol

The protocol uses explicit pending states rather than exposing partial contacts
as usable saved devices:

- `PendingOutgoing`
- `PendingIncoming`
- `Saved`

The normal exchange is:

1. Alice locally chooses to remember Bob after a qualifying transfer.
2. Alice sends a token-bound pairing request.
3. Bob explicitly consents.
4. Alice and Bob exchange fresh directional grants.
5. Alice acknowledges Bob's grant.
6. Both sides activate the relationship as `Saved` only after the mutual
   exchange is acknowledged.

Failure before activation remains a bounded pending operation and cannot be
used to initiate a transfer. Pending operations expire and are recoverable or
cleaned after crashes.

If both devices initiate simultaneously, the protocol deterministically merges
the attempts using the endpoint identities and the transfer-bound eligibility
capability. It creates one relationship and one active grant per direction,
without duplicate prompts or rows.

Declining consumes the eligibility for that qualifying transfer. It cannot
prompt again. A later completed transfer may establish new eligibility, but
another request still requires fresh local initiation.

---

## 7. Directional grants

Each direction has one active, high-entropy capability bound to:

- Issuer endpoint identity.
- Holder endpoint identity.
- Relationship generation.
- Minimum negotiated protocol generation.

Proof uses the authenticated iroh channel plus established, domain-separated
cryptographic primitives, challenge binding, and replay protection. Display
names, addresses, and transfer IDs alone are never authentication. The protocol
must have independent, reviewable test vectors.

Relationships do not expire merely through inactivity. They remain until
forget, block, explicit revocation, identity loss, or reset. Long-unseen devices
may later be represented as inactive by UI, but inactivity does not silently
remove permission.

Activating a replacement grant first makes the prior relationship generation
locally invalid. Exactly one generation is active per direction. Minimal
non-secret revocation tombstones are retained for as long as an old generation
could otherwise be replayed; tombstones contain no names, filenames, transfer
history, or capability material.

An established relationship records its minimum supported protocol generation
and must never silently downgrade below it.

---

## 8. Forget, block, and identity replacement

### Forget

Forget makes the local relationship and its grants unusable immediately,
cancels active or resumable targeted transfers for that relationship, removes
relationship secrets and metadata, and sends a signed/bound best-effort remote
revocation when possible. Correctness never depends on remote delivery.

An independently approved invitation transfer already in progress may continue
because it belongs to the existing share domain.

### Block

Block is identity-wide and immediate. It rejects or cancels current and future
traffic from the blocked endpoint across:

- Pairing and grant operations.
- Targeted offers and transfers.
- Ordinary invitation handshakes and transfers.
- Revocation and probing endpoints, except for indistinguishable rejection
  needed to avoid exposing block state.

Blocking deletes active relationship grants but retains the minimal identity
deny record and replay tombstones. Unblocking removes only the deny rule. It
does not restore grants, relationships, or cancelled transfers. Saving the
device again requires another qualifying transfer and fresh mutual consent.

A peer reinstall produces a new endpoint identity. It is never linked to the
old device by name, address, or platform. The old saved entry remains
unavailable until forgotten; the new identity follows the complete first-
transfer and consent flow.

---

## 9. Targeted-transfer model

`TargetedTransfer` is not an access mode on an ordinary share. It has its own
protocol types, repository records, authorization rules, and public APIs.
Internal blob storage, import, hashing, streaming, and output-sink machinery may
be reused.

The following fields are immutable after creation:

- Transfer ID.
- Sender endpoint identity.
- Receiver endpoint identity.
- Manifest identity and content hashes.
- File count and total size.

Sending identical content to several saved devices creates independent
targeted transfers. Internal blobs may be deduplicated, but approval, progress,
cancellation, retry, authorization, and durable state remain independent.

The durable state machine is:

```text
Preparing -> Offering -> AwaitingApproval -> Approved -> Connecting
          -> Transferring -> Completed
                         \-> Interrupted -> Connecting

Terminal alternatives: Declined, Cancelled, Failed, Deleted
```

Rust centrally validates transitions. Platform code invokes typed operations
and consumes snapshots/events; it cannot fabricate states.

---

## 10. Offer and approval protocol

An offer is online-only and bounded:

1. The sender creates an immutable targeted transfer for one saved-device
   identity.
2. The peers authenticate the saved relationship and negotiate the targeted-
   transfer protocol version.
3. The sender submits a bounded offer containing a stable transfer ID and an
   authenticated manifest summary, but no reusable ordinary-share ticket.
4. The receiver validates all framing, limits, identity bindings, relay-policy
   compatibility, and manifest claims before surfacing approval.
5. The receiver explicitly approves or declines.
6. On approval, the sender issues authorization bound to the exact transfer,
   manifest, and receiver endpoint.
7. The receiver pulls the content through the existing safe streaming and
   output-sink machinery.

The approved authorization covers the exact manifest, content hashes, sizes,
sender, receiver, transfer ID, and protocol generation. Any mismatch or content
mutation invalidates the transfer and requires a new transfer ID and approval.
A leaked capability must fail when presented by another endpoint.

Every operation is idempotent. Replaying the same pairing request, offer,
approval, acknowledgement, cancellation, or completion returns the existing
result and cannot create duplicate prompts, grants, authorizations, or rows.

Declining rejects only that transfer. It neither forgets nor blocks the sender.

---

## 11. Online, interruption, and deletion semantics

An unapproved offer exists only in a bounded live-session queue. Sender
cancellation, decline, timeout, disconnect, or core restart removes it. There
is no sender-held offline offer, receiver polling loop, background inbox, or
automatic retry that can produce a later prompt.

After approval, the transfer and its recipient-scoped authorization are
durable. Interruption retains verified progress and may resume when both devices
are online again. Resuming the same immutable transfer does not request another
approval. Changed content or metadata requires a new transfer.

Cancellation before approval withdraws the offer. Cancellation after approval
stops authorization and active streaming synchronously before asynchronous
cleanup. It affects only that transfer.

Deletion must make authorization unusable, stop content service for that
transfer, remove resumable state, and clean related secrets. Remote cleanup is
best-effort; immediate durable local denial is mandatory.

Several separately approved targeted transfers may run concurrently between
the same devices under existing global stream and resource limits.

---

## 12. Local data and consistency

The private application database may contain only the relationship and transfer
metadata needed for the feature, including:

- Endpoint identity/public identifier.
- User-owned local label and untrusted platform/name hints.
- Pending/saved/blocked/revoked state and state revision.
- Opaque secure-store handles.
- Protocol and relationship generation.
- Last successful authenticated contact time.
- Minimal replay and revocation tombstones.
- Durable targeted-transfer state after approval.

It must not become a transfer-history log. Pairing does not justify retaining
filenames, previous IP addresses, or lists of past transfers.

Credential-store and SQLite updates cannot share a native transaction. Use
recoverable staged transitions:

1. Write secret material under a versioned opaque handle.
2. Verify the protected write.
3. Commit metadata referencing that handle in a non-active state.
4. Finalize activation.

Startup reconciliation removes orphaned secrets and disables metadata whose
required secrets are missing. Revocation becomes locally effective before any
network notification. Relationship mutations are serialized per remote
endpoint, while unrelated devices proceed concurrently. Database, relationship,
and credential-store guards must never be held across network awaits.

---

## 13. Core and platform contract

The Rust core exposes separate typed models and operations for:

- Pairing eligibility and pending pairing requests.
- Listing, renaming, forgetting, blocking, and unblocking saved devices.
- Creating and submitting targeted transfers.
- Approving, declining, cancelling, resuming, and deleting transfers.
- Querying durable state and current capability availability.
- Subscribing to typed events carrying stable IDs and monotonic state revisions.

Bindings must not expose raw secrets or generic state mutation. Events are
wake-up notifications, not authoritative storage. They may be delivered at
least once; consumers deduplicate by stable ID and revision, then query current
state after reconnect or restart.

### 13.1 Pairing and targeted-transfer event catalog

Canonical kinds emitted on `CoreEvent` (phase → kind). Treat every event as a
wake-up: refresh durable state via list/get APIs. Mid-transfer progress polish
(live `verified_bytes` updates) may follow; this catalog is the readiness bar.

**`pairing`**

| Kind | Meaning |
|---|---|
| `eligibility-available` | Pairing eligibility exists for a peer after a completed authenticated invitation transfer. |
| `eligibility-removed` | Eligibility expired or was consumed/removed. |
| `relationship-changed` | Device-relationship state changed (pending, saved, revoked, blocked). Payload includes peer id and state. |
| `relationship-grant-rotated` | Local relationship grant generation advanced for a peer. |
| `saved-device-forgotten` | Local forget completed for a saved peer. |
| `device-blocked` | Peer was blocked locally. |

**`targeted_transfer`**

| Kind | Meaning |
|---|---|
| `offer-received` | A pre-approval offer is pending local approve/decline. |
| `approved` | Local approval completed; authorization is in core custody. |
| `offer-declined` | Local decline completed. |
| `created`, `offering`, `awaiting-approval` | Sender-side durable setup and offer lifecycle changed. |
| `connecting`, `transferring`, `progress`, `interrupted` | Receiver-side pull lifecycle or verified payload progress changed. |
| `completed`, `cancelled`, `failed`, `deleted` | A durable targeted-transfer terminal snapshot changed. |

Lifecycle payloads use `targeted_transfer_id`; consumers refresh the corresponding
snapshot after receiving the wake-up. Progress payloads remain advisory and the
durable snapshot is authoritative.

Failures remain typed where callers can act differently, including:

- Device unavailable or offer timeout.
- Protocol incompatibility or forbidden downgrade.
- Revoked or blocked relationship.
- Relay-policy incompatibility.
- Secure storage locked, unavailable, missing, or corrupted.
- Approval decline, cancellation, interruption, and invalid transition.

Production errors and diagnostics must not expose endpoint IDs, direct
addresses, tickets, grants, pairing capabilities, filenames, or secret-store
payloads.

---

## 14. Limits and hostile-peer handling

A saved relationship proves a remote app identity and permits it to request
approval. It does not make remote metadata, filenames, paths, sizes, messages,
or content trusted.

The feature reuses all existing filesystem safety, output-sink, no-overwrite,
ticket validation, and resource-limit invariants. Before approval it also
enforces:

- One unresolved offer per sender identity.
- A bounded global pending-offer queue.
- Strict request, manifest, metadata, file-count, and size limits.
- Connection, pairing, offer, approval, and acknowledgement timeouts.
- Per-identity cooldown after repeated malformed traffic or declines.
- Silent rejection of unauthenticated, ineligible, blocked, or invalid traffic.
- A configurable `CoreLimits.max_saved_devices`, defaulting to 256.

These are control-plane and local-resource protections. They do not impose a
quota on accepted transfers, files, bytes, or bandwidth.

VniDrop cannot protect against a compromised or unlocked endpoint, malicious
files the receiver knowingly accepts, operating-system credential compromise,
network traffic analysis, or a reinstalled peer appearing under a new identity.

---

## 15. Compatibility and release policy

Saved devices and targeted transfers use explicit, versioned protocol
capabilities. A peer without compatible support cannot be paired or receive a
targeted transfer and falls back to the existing invitation flow. A targeted
transfer must never be reinterpreted as an ordinary share for compatibility.

The feature is gated as experimental in the 0.3.x line. The wire protocol is
versioned from its first merge. Removing the experimental gate requires:

- Stable migrations from every released database version.
- Compatible Apple, Android, Windows, and Linux credential-store adapters.
- Rust and platform contract coverage.
- Stable downgrade, revocation, recovery, and lifecycle behavior.
- No regression in invitation-based multi-recipient transfers.

The unreleased `feat/device-history` contact schema, held offers, polling
behavior, expiring grants, `Contact` terminology, Apple-only feature UI, and
ordinary-share offer authorization were prototype artifacts and have been
removed without a compatibility migration. Useful low-level cryptographic,
repository, protocol, and test patterns were retained only after they were
checked against this design.

---

## 16. Verification requirements

Rust tests must deterministically cover:

- Mutual consent, decline, simultaneous initiation, timeouts, and lost
  acknowledgements.
- Pairing eligibility after completion and rejection after every non-completed
  outcome.
- Replay, malformed input, spoofed identity, blocking, revocation, grant
  rotation, and protocol downgrade.
- Recipient-bound authorization and rejection of leaked capabilities.
- Direct, relay, custom-relay, local-only, and incompatible-profile behavior.
- Restart and recovery at every durable state.
- Cancellation, deletion, forget, and block during active streaming.
- Credential-store failure and crash-point reconciliation.
- Concurrent independent targeted transfers.
- Existing invitation-based multi-recipient behavior remaining unchanged.

Each platform secure-storage adapter requires contract coverage for create,
read, update, delete, locked/unavailable behavior, migration, device-bound
persistence, orphan cleanup, and redaction. Platform harnesses must prove that
secrets do not appear in generated bindings, logs, diagnostics, or ordinary
database columns.

The core/platform foundation is complete only when these contracts are
implemented, documented, exposed through typed UniFFI APIs, and pass the
relevant Rust and platform checks. UI polish is not part of that completion
boundary.
