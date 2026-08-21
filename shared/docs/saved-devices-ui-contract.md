# Saved devices UI contract (KMP + Apple)

Product UI for saved devices and targeted transfers. Rust UniFFI is the source
of behaviour; this document keeps Apple and KMP interaction semantics aligned.

Status: production, top-level product surface.

## Vocabulary

Use **saved device**, **device relationship**, **targeted transfer**, **invitation transfer**. Do not say contact, person, or account in UI copy.

## Product surface

- KMP Android, Windows, and Linux expose Saved Devices as a top-level
  destination. It is not controlled by an experimental preference.
- Apple exposes Saved Devices in the native iOS tab bar and macOS sidebar.
- The populated main screen lists saved devices and outstanding consent
  requests. Targeted Transfer history belongs to the selected device's detail
  surface, not the global list.

## Events are wake-ups

`pairing` and `targeted_transfer` core events (and KMP `CoreSignal.PairingChanged` / `TargetedTransferChanged`) mean: refresh durable state via list/get APIs. Do not treat event payloads as authoritative storage. Deduplicate by event id/revision when needed.

Relevant kinds:

- **pairing:** `eligibility-available`, `eligibility-removed`, `relationship-changed`, `relationship-grant-rotated`, `saved-device-forgotten`, `device-blocked`
- **targeted_transfer:** `offer-received`, `approved`, `offer-declined`, `created`,
  `offering`, `awaiting-approval`, `connecting`, `transferring`, `progress`,
  `interrupted`, `completed`, `cancelled`, `failed`, `deleted`. Domain contract
  v2 transports no transfer identity in these wake-ups; refresh list/get after
  each event.

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
   - Android KMP: the MediaStore Downloads sink used by Invitation receives.
   - Windows / Linux KMP: configured filesystem receive folder path when no output sink is provided.

## Targeted lifecycle

The selected device's detail surface owns receive, resume, cancel, delete,
progress, and terminal Targeted Transfer states. Background approval
notifications are wake-ups into the same durable-state refresh path.

Label, forget, and block actions are available from Saved Devices. Label edits
preserve the draft and editor on failure and close only after a successful save.
