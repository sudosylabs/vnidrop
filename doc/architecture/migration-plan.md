# Architecture migration plan

## Goal

Move Targeted transfer lifecycle authority into one deep Rust module, give
callers a preparation interface that returns durable identity promptly, and
derive platform retention and read models from authoritative core facts.

The migration was additive through domain contract v1. Domain contract v2
removes the superseded blocking creation entry point and Targeted event identity
payload after both platforms migrated to authoritative reads.

## Parallelism rule

Work can run in parallel when tracks have different source ownership and agree
on fixtures or contracts first. Changes to the same lifecycle authority must be
serialized. In particular, protocol callback collapse, durable transition
semantics, and the new public interface are one dependency chain rather than
independent candidates.

Each wave ends with an integration gate before dependent work begins.

## Wave 1: foundations

Three tracks can run in parallel.

### Core characterization and identity

- Add deterministic characterization tests for current Targeted creation,
  approval, cancellation, failure, restart, and provider authorization.
- Establish the immutable sender, receiver, manifest, and content identity
  committed at durable registration.
- Define private fault adapters for store, negotiation, and timing failures.

### Platform read-model characterization

- Inventory Kotlin and Swift relationship/transfer joins and action derivation.
- Create one canonical scenario matrix shared conceptually by both platforms.
- Begin extracting in-process Saved Devices read-model modules without changing
  the core contract.

### Retention contract and adapter hardening

- Encode the Runtime obligation scenario matrix.
- Move Android retention ownership from composable lifetime to application-graph
  lifetime.
- Make the platform adapter idempotent and teardown-safe while it still consumes
  the existing facts.

### Gate

Current lifecycle behavior, read-model scenarios, and retention scenarios are
covered before structural migration begins.

## Wave 2: concentrate core authority

Status: implemented on `feat/device-history`; the gate commands remain the
source of truth before integration.

- Introduce the deep internal Targeted transfer module.
- Move durable transitions, authorization custody, cleanup, recovery, and
  approval/stop race linearization behind its interface.
- Reduce the Targeted wire protocol to a narrow translation adapter.
- Keep the device-relationship module separate and consult it through a private
  seam.
- Preserve public UniFFI behavior through the existing facade.

Platform read-model extraction from Wave 1 may continue in parallel because it
depends on stable durable reads, not the new creation interface.

### Gate

The internal module tests and existing public contract tests pass with no
platform migration required.

## Wave 3: lifecycle interface and obligations

Status: implemented on `feat/device-history`; the public contract and full Rust
gates are the integration source of truth.

- Add `newTargetedTransferPreparation(receiver)`.
- Make `preparation.send(sources, name)` return the durable Targeted transfer
  after registration while core-owned negotiation continues.
- Make `preparation.stop()` return a precise outcome for stopped preparation,
  abandonment, cancellation, or an already-terminal transfer.
- Expose neutral Runtime obligation facts, including targeted preparation and
  provider availability.
- Implement the old blocking creation call as a compatibility adapter.

### Gate

Public contract tests cover prompt identity return, concurrent stop, the
approval/abandon race, restart recovery, and compatibility behavior.

## Wave 4: platform migration

Status: implemented on `feat/device-history`; the cross-platform gate below is
the integration source of truth.

Kotlin and Swift migrations can run in parallel against the gated core contract.

### Kotlin/Compose

- Adopt the preparation interface and direct durable identity return.
- Finish the Saved Devices read-model module and remove presentation overlays.
- Feed Runtime obligation facts to the application-graph retention owner.
- Stop scraping Targeted transfer IDs from event JSON.

### Swift/Apple

- Adopt the same lifecycle contract and canonical scenario matrix.
- Finish the Apple Saved Devices read model.
- Map Runtime obligations to Apple lifecycle facilities without coupling them to
  notification presentation.
- Stop scraping Targeted transfer IDs from event JSON.

### Gate

Rust, shared KMP, and Apple contract scenarios agree on identity, action facts,
stop outcomes, retention, missed events, and restart behavior.

## Domain contract v2 cleanup

Status: implemented after the Wave 4 cross-platform gate.

- Remove the blocking `create_targeted_transfer` UniFFI entry point.
- Keep `new_targeted_transfer_preparation` as the single sender creation path.
- Remove `targeted_transfer_id` from Targeted event JSON. Events are refresh
  hints; callers obtain identities from preparation results and durable reads.
- Advance `domain_contract_version` to `2` without changing either wire protocol
  version.

The migration and its compatibility cleanup are complete.

## Verification strategy

- Put most lifecycle coverage at the deep Rust module boundary.
- Keep a smaller public UniFFI suite for cross-language compatibility.
- Test platform read models with the same canonical scenario matrix.
- Use deterministic gates and fault adapters instead of long sleeps.
- Run repository checks required by the files changed in each migration slice;
  do not defer cross-layer verification to the final wave.
