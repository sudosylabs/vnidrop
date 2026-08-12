# Saved devices UI contract (KMP + Apple)

Experimental product UI for saved devices and targeted transfers. Rust UniFFI is the source of behaviour; this document keeps Apple and KMP interaction semantics aligned.

Status: experimental (default off in KMP Settings).

## Vocabulary

Use **saved device**, **device relationship**, **targeted transfer**, **invitation transfer**. Do not say contact, person, or account in UI copy.

## Experimental gate

- KMP Android / Windows / Linux: Settings → Experimental → Saved devices, preference default **off**, persisted.
- KMP generic desktop host (`UiPlatform.Desktop`, e.g. macOS JVM): experimental UI **hidden**.
- Apple: gate shape is platform-owned; semantics below still apply when the feature is enabled.

## Events are wake-ups

`pairing` and `targeted_transfer` core events (and KMP `CoreSignal.PairingChanged` / `TargetedTransferChanged`) mean: refresh durable state via list/get APIs. Do not treat event payloads as authoritative storage. Deduplicate by event id/revision when needed.

Relevant kinds:

- **pairing:** `eligibility-available`, `eligibility-removed`, `relationship-changed`, `relationship-grant-rotated`, `saved-device-forgotten`, `device-blocked`
- **targeted_transfer:** `offer-received`, `approved`, `offer-declined`, `created`,
  `offering`, `awaiting-approval`, `connecting`, `transferring`, `progress`,
  `interrupted`, `completed`, `cancelled`, `failed`, `deleted`. Durable-row
  wake-ups include `targeted_transfer_id`; refresh list/get after each event.

## Pairing

After a completed invitation transfer, eligibility may exist. The user may accept or decline. Mutual consent yields a saved device. Pending eligibility/relationship state must remain reachable if an in-flow prompt is dismissed.

## Targeted approve / pull

1. List pending targeted offers (or react to `offer-received`).
2. `respond_to_targeted_offer(transfer_id, accepted)` → typed outcome only:
   - `Approved { transfer_id }`
   - `Declined`
   - `AlreadySettled { transfer_id }`
3. Never accept or display authorization/grant strings across the public binding.
4. Pull / resume with **transfer id + destination** (path or output sink).
   - Android KMP: invitation MediaStore Downloads sink for the experimental MVP receive.
   - Windows / Linux KMP: configured filesystem receive folder path when no output sink is provided.

## Out of this contract’s MVP chrome

Resume / cancel / delete / grant-rotate UI, background notifications, and mid-transfer progress polish may follow without changing the approve/pull rules above.

Block is available from the Saved devices area. Unblock is not required for the KMP MVP chrome when the product surface does not already expose blocked-device management.
