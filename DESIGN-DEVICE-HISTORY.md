# Design — Device history and direct offers

Status: **draft for review**. No code written.

Lets a user send to a device they have already transferred with, without
creating and sharing a new invitation. Both sides opt in to being remembered,
and either side can end the relationship later and have that actually take
effect on the other device.

Local network discovery was considered and deliberately dropped. See
[Appendix A](#appendix-a--deferred-local-network-discovery).

---

## 1. Goals and non-goals

### Goals

- Send to a previously used device with no new invitation, QR code, or NFC tap.
- Let each side independently decide whether to be remembered after a transfer.
- Let either side revoke that relationship unilaterally, with real effect.
- Keep the receiving side's confirmation mandatory for every transfer that
  arrives this way.

### Non-goals

- No automatic acceptance of transfers, under any configuration.
- No store-and-forward: an offer to an unreachable device fails; nothing is
  queued and nothing is uploaded anywhere.
- No presence or "who is online" indicator. Knowing it requires probing, and
  probing tells every contact when you opened your list. Reachability is
  resolved lazily, at send time.
- No change to the invitation (QR / NFC / `.vnd`) flow, which remains how a
  first contact is made and how an unpaired device is reached.

### Relationship to the existing flow

First contact is unchanged: an invitation, a transfer, a receiver confirmation.
This feature only removes the invitation step from the *second* and subsequent
transfers between the same two devices.

---

## 2. Threat model

Assume an attacker who can run a modified VniDrop client, choose any display
name, and reach the target over the network.

| Property | Mechanism |
|---|---|
| A stranger cannot send an unsolicited transfer prompt | The offer protocol requires a valid grant (§3) |
| A stranger cannot impersonate a known device | Identity is the iroh endpoint key; display names are untrusted data |
| Being remembered requires consent from the remembered party | Grants are minted by the party being remembered (§3.4) |
| A user can end a relationship unilaterally | A grant is validated only by its issuer (§3.3) |
| A revoked peer cannot quietly regain access | Revocation is local and immediate; no cooperation required |

Explicit non-property: we cannot erase data from a device we do not control. A
revoked peer's app may still hold a name string on disk. What is guaranteed is
that the entry stops **functioning** — see §3.3.

The app broadcasts nothing and advertises nothing. There is no passive network
surface introduced by this feature at all.

---

## 3. Grants: the core primitive

A history entry is **not** "I remember this device's endpoint ID". It is "this
device issued me a capability to reach it". This is what makes both consent and
revocation real rather than promised, and it is the reason a grant-based design
is worth the modest extra complexity over storing a public key.

### 3.1 Shape

A grant is directional. If Alice wants Bob to be able to reach her, *Alice*
mints the grant and gives it to Bob:

- `grant_id` — 128-bit random, opaque.
- `grant_secret` — 256-bit random.
- Bound to Bob's endpoint ID at issue time.
- `expires_at` — an **idle** expiry, renewed on use (§3.5).

Alice keeps `(grant_id, grant_secret, bob_endpoint_id, expires_at, revoked_at)`
in her **issued** table. Bob keeps `(grant_id, grant_secret, alice_endpoint_id,
display_name, …)` in his **held** table, which is what his history UI lists.

A mutual relationship is two independent grants. Either side can revoke its own
without affecting the other direction, which is the correct semantics: "you may
no longer reach me" is separable from "I may no longer reach you".

### 3.2 Proving a grant

iroh already provides a mutually authenticated, encrypted QUIC connection, so
both endpoint IDs are known and trustworthy at the transport layer. On top of
that, challenge–response proves possession of the grant without ever
transmitting it:

1. Alice (the accepting side) sends a 32-byte random `challenge`.
2. Bob replies with `grant_id` and
   `HMAC(grant_secret, "vnidrop-grant-v1" ‖ challenge ‖ alice_endpoint_id ‖ bob_endpoint_id)`.
3. Alice looks up `grant_id`, checks it is neither revoked nor expired, checks
   that the connection's remote endpoint ID equals the endpoint the grant was
   issued to, and verifies the HMAC in constant time.

Binding to the issued-to endpoint means Bob cannot lend his grant to a third
party. Binding to the challenge means a captured proof cannot be replayed.

### 3.3 Revocation

Alice deletes (or tombstones) the grant in her issued table. That is the whole
mechanism, and it is sufficient: hers is the **only** device that can validate
it. Bob's next attempt presents an unknown `grant_id`, is refused, and his
client deletes the dead entry.

The refusal is **explicit**: ordinary revocation returns a distinct `Revoked`
status so Bob's client can remove the entry immediately and tell him the device
is no longer available. Silence would leave a zombie entry, and Bob can infer
what happened regardless, so the deniability is not worth the worse behavior.

The hard block list is the exception: a blocked endpoint receives a response
indistinguishable from an expired or unknown grant, so blocking cannot be
detected by probing.

Additionally, when Alice revokes while Bob is reachable, she sends a best-effort
`RevokeGrant { grant_id }` so his entry disappears promptly rather than at his
next attempt. Best-effort only — correctness never depends on it arriving.

Two things revocation deliberately is **not**:

- **Not retroactive.** Files already sent stay sent. UI copy must say so.
- **Not a block.** Bob can still reach Alice with a QR invitation like any
  stranger. A separate hard block list refuses a given endpoint ID at the offer
  and handshake layers.

### 3.4 Consent to be remembered

After a completed transfer, each side is asked independently whether to remember
the other. If Alice declines, no grant is minted, so Bob has nothing functional
to store and his UI must not offer to save the device. Bob cannot override
Alice's choice, because the useful half of the entry is hers to issue.

The prompt is per-transfer and must be dismissible without a choice, defaulting
to "no". A user who never engages with it is never added to anyone's history.

### 3.5 Grant lifetime

Grants expire on **idleness, not age**. Each successful offer renews the
issuer's `expires_at`, so a relationship in regular use never lapses, while one
that is forgotten cleans itself up.

Default idle lifetime: **90 days**, configurable per device in settings
(30 / 90 / 365 days / never) and applied at issue time. Changing the setting
affects newly minted grants; existing ones keep the lifetime they were issued
with until renewed.

Renewal is issuer-side only and needs no protocol message: Alice extends the
grant when she validates a proof from Bob. An expired grant behaves exactly like
a revoked one from Bob's side, except that the UI explains it as inactivity and
offers to pair again rather than presenting it as a deliberate removal.

This bounds the blast radius of a pairing the user has forgotten about, and it
softens the reinstall problem in §5.1: dead entries pointing at a regenerated
`iroh.secret` eventually disappear on their own.

---

## 4. The offer protocol

Today the protocol is strictly receiver-pull: the sender never initiates. An
offer inverts only the *delivery of the ticket*, not the transfer itself.

New ALPN: `/vnidrop/offer/1`.

1. Sender picks a contact from history.
2. Sender creates the share exactly as today (`share_files`). The share is
   `ApprovalRequired`; an offer-created share may **never** be `Public`
   (invariant, enforced in `access_policy`).
3. Sender pre-authorizes the target endpoint for that `transfer_id` via the
   existing `AccessPolicy::approve_endpoint_until`, so the sender is not later
   prompted to approve a transfer they themselves initiated.
4. Sender dials the target's offer ALPN, completes the grant challenge–response
   (§3.2), and sends
   `Offer { ticket, sender_display_name, file_count, total_bytes }`.
5. **The receiver is prompted.** This is the mandatory confirmation and it has
   no bypass.
6. On accept, the receiver calls the existing `receive(ticket, output_dir,
   receiver_name)` — completely unchanged. It dials the sender's existing
   `/vnidrop/handshake/2`, where the pre-authorization from step 3 is already in
   place, so exactly one human is prompted for the whole flow.
7. On decline, the sender receives `Declined` and stops the share.

The ticket must satisfy the receiver's relay profile, so the existing
`ticket_matches_relay_profile` check applies unchanged: a contact on a
strict-custom profile will refuse an offer whose ticket advertises public
relays, and the UI must explain that rather than failing opaquely.

### 4.1 Identity display

Display names are attacker-chosen data — the existing handshake already treats
`receiver_name` that way, and the same rule applies here. The endpoint ID is the
only real identity. Therefore:

- A contact's local label is set by the local user and is **never** silently
  overwritten by a name the remote later claims. A changed remote name is shown
  as a distinct, dismissible signal.
- A short fingerprint derived from the endpoint ID is available in the contact
  detail view, for out-of-band verification.

---

## 5. Address resolution and reachability

A contact stores an endpoint ID, but iroh needs an address to dial. Without
local discovery, resolution depends on the relay profile:

| Relay mode | Resolution |
|---|---|
| `Automatic` | Public discovery resolves the endpoint ID anywhere |
| `StrictCustom` / `CustomWithDirectFallback` | Reachable through the configured relay, whose URL is stable |
| `LocalOnly` | Only while the cached direct address is still valid |

`presets::Minimal` deliberately leaves address lookup empty for the restricted
modes (see the comment at `runtime/mod.rs:154`), so those modes cannot fall back
to public resolution — by design.

**Mitigation: cache the peer's last-known `EndpointAddr` on the contact and
refresh it after every successful connection.** The repository already persists
sender addresses this way for receive rows —
`encode_persisted_sender_address` / `parse_persisted_sender_address` in
`ticket.rs:72` — so this reuses an established pattern rather than inventing
one.

This covers relay modes fully, and covers `LocalOnly` for as long as the peer's
address is unchanged. When it is not, the send fails and the user falls back to
a QR invitation: no regression against today's behavior, but the UI must say so
plainly rather than presenting an opaque failure. Local-only users in particular
should be told that contacts depend on a cached address.

Reachability is never polled in the background. It is determined when the user
actually sends.

### 5.1 Identity lifetime

Reinstalling the app regenerates `iroh.secret`, so every grant referencing the
old endpoint dies. The UI needs an explicit "this device is no longer
recognized, pair again" state rather than a silent failure.

---

## 6. Data model

New tables in the existing SQLite repository, with a schema migration:

| Table | Columns (sketch) |
|---|---|
| `contacts` | `id`, `endpoint_id` (unique), `local_label`, `remote_display_name`, `last_known_addr`, `created_at`, `last_transfer_at` |
| `grants_issued` | `grant_id`, `grant_secret`, `issued_to_endpoint_id`, `created_at`, `expires_at` (idle, renewed on use), `revoked_at` |
| `grants_held` | `grant_id`, `grant_secret`, `peer_endpoint_id`, `created_at`, `expires_at` (advisory copy) |
| `blocked_endpoints` | `endpoint_id`, `created_at` |

`grant_secret` is **key material**. It follows the same rule as tickets: never
in events, never in logs, never in bug reports, never in a UniFFI return value.
The existing "tickets are capabilities" discipline extends verbatim.

A contact list is itself a privacy artifact — it names the people someone
exchanges files with. It must be deletable per-entry and wholesale, and the
wholesale delete must be reachable from the same place as the existing
transfer-history and cache clearing actions.

Deleting a contact deletes both directions' grants for that peer and, for the
issued side, triggers the best-effort revoke message.

---

## 7. Abuse and resource limits

Extend `CoreLimits` rather than inventing a parallel mechanism:

- `max_contacts`.
- `max_pending_offers`, mirroring the existing `max_pending_approvals`.
- Per-endpoint offer rate limiting, with a cooldown after repeated declines.
- Blocked endpoints are refused at the offer ALPN before any user-visible
  prompt.

Because an offer already requires a valid grant, the spam surface is limited to
devices the user deliberately chose to be reachable by, and the remedy — revoke
— is one tap.

---

## 8. Surfaces to build

- **Rust core:** offer ALPN and handler, grant minting/proof/revocation,
  contacts and grants repository with migration, address caching, new limits,
  block list.
- **UniFFI:** additive API — list/rename/delete contacts, send-to-contact,
  revoke, block/unblock, respond to an incoming offer, plus the corresponding
  events. Additive changes do not break existing Kotlin or Swift call sites, but
  both must be updated to use them.
- **Compose (`shared/`)** and **SwiftUI (`apple/`)**: a contacts list and detail
  view, the post-transfer "remember this device?" prompt, the incoming-offer
  confirmation, a send-to-contact entry point in the send flow, and settings for
  the feature toggle, the grant idle lifetime (30 / 90 / 365 days / never,
  default 90), and blocked devices.
- **Localization:** all new strings go in `localization/strings.json` and are
  generated; the platform catalogs are never hand-edited.

No new OS permissions, entitlements, or platform bridges are required.

---

## 9. Testing

- **Grant crypto:** fixed vectors for the HMAC proof; expiry, revocation,
  wrong-endpoint binding, and replay rejection.
- **Grant lifetime:** a successful proof renews `expires_at`; an idle grant
  lapses at the configured boundary; a renewed grant survives past its original
  expiry. Assert the revoked and blocked responses are distinguishable from each
  other and that blocked is indistinguishable from expired/unknown.
- **Offer protocol:** two in-process nodes using the existing
  `crates/vnidrop/tests/support` harness — accept, decline, revoked grant,
  expired grant, blocked endpoint, relay-profile mismatch, and the invariant
  that an offer-created share is never `Public`.
- **Pre-authorization:** assert the sender is prompted exactly zero times and
  the receiver exactly once, for a full offer → accept → transfer round trip.
- **Consent:** assert that declining to be remembered leaves the peer with no
  usable grant, and that a subsequent offer from that peer is refused.
- **Address caching:** a contact whose cached address is stale falls back
  cleanly and reports an actionable error, rather than hanging.
- **Persistence:** grants and contacts survive a core shutdown and reopen of the
  same data dir, following the existing recovery-test pattern.
- Per `AGENTS.md`, any bug found gets a regression test at the lowest layer.

---

## 10. Settled decisions

Both previously open questions are decided and specified above; recorded here
with their rationale so the reasoning is not lost.

1. **Revocation is reported explicitly** (§3.3). A revoked peer's client
   receives a distinct status and removes the dead entry immediately. The
   alternative — silence — leaves a zombie entry, and the revocation is
   inferable from the failure anyway, so the deniability is illusory.
   Indistinguishable silence is reserved for the hard block list, where
   undetectability is the point.
2. **Grants expire on idleness, renewed on use, defaulting to 90 days** (§3.5),
   configurable to 30 / 90 / 365 days or never. Relationships in regular use
   never lapse; forgotten ones clean themselves up, which bounds the blast
   radius of a stale pairing and quietly disposes of entries orphaned by a
   reinstall.

---

## Appendix A — Deferred: local network discovery

An earlier draft specified AirDrop-style discovery: three visibility tiers
(invisible / paired-only / a time-boxed pairing window), private per-grant mDNS
beacons using rotating per-epoch AEAD entries so only grant holders could
recognize a device, and a short-authentication-string pairing flow. It was
dropped, because once first contact requires a completed transfer anyway,
discovery adds far less than it costs.

**What it would have added:** camera-free pairing (QR pairing already works),
live presence (which requires probing, and probing leaks when a user opens their
contact list), and address resolution on a network with no public discovery —
the only substantive one, and largely handled by the address caching in §5.

**What dropping it avoids:**

- The `com.apple.developer.networking.multicast` entitlement risk. iroh's
  local-network discovery uses raw multicast sockets rather than Bonjour, and
  that entitlement requires a special request to Apple that is frequently
  refused. This was the single largest threat to shipping.
- Local network permission prompts on iOS/macOS, an Android multicast lock and
  `NEARBY_WIFI_DEVICES`, a Windows firewall prompt, and avahi coexistence on UDP
  5353.
- A per-platform discovery bridge, including a native `NWBrowser`/`NWListener`
  implementation in Swift.
- Beacon crypto, epoch/clock-skew handling, and a hard cap of roughly 24–28
  advertised contacts imposed by the mDNS packet budget.
- A contradiction with the README's promise that the restricted relay modes
  never use "public discovery".
- Visibility-tier settings, which are difficult to explain and easy to
  misconfigure.

It also *improves* the privacy posture: the app broadcasts nothing at all, which
is a stronger and far more explainable claim than any beacon scheme, including
in an App Store review.

**Network-trust detection was rejected separately and stays rejected.** Deciding
what to expose based on whether a network looks "public" is unreliable — macOS
has no such concept, Android needs `ACCESS_FINE_LOCATION` to read an SSID, and
iOS cannot identify the current network at all without
`com.apple.developer.networking.wifi-info` plus location permission. It is also
spoofable, since an attacker can clone an SSID and choose a gateway MAC.

**If it is ever revisited**, the beacon scheme was deliberately keyed off grants,
so it layers onto the tables in §6 with no change to the offer protocol or the
data model. Nothing in this design forecloses it. One unrelated cleanup noted
along the way: `apple/VniDrop/Resources/Info.plist:78` declares
`NSBonjourServices` with a single empty-string entry, which is meaningless and
should be removed or given a real service type.
