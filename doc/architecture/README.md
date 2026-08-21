# VniDrop architecture

This handbook explains VniDrop's durable architectural boundaries and the
direction selected for the Targeted transfer migration. It is organized by
topic so a contributor can understand one subsystem without first reading the
entire repository.

Architecture statements use these labels:

- **Current** describes behavior implemented in the repository today.
- **Target** describes an agreed design that has not necessarily landed yet.
- **Invariant** is a constraint current and future implementations must preserve.

## Reading order

| Topic | What it answers |
|---|---|
| [System overview](system-overview.md) | Which layer owns each responsibility? |
| [Transfers](transfers.md) | How do Invitation and Targeted transfers differ? |
| [Connections](connections.md) | How are peers authenticated and reached? |
| [Iroh](iroh.md) | What does Iroh provide and how is it constrained? |
| [Persistence](persistence.md) | Which state is durable and authoritative? |
| [Security](security.md) | Where are trust, authorization, and file safety enforced? |
| [Platforms](platforms.md) | What belongs in Rust, Kotlin, Swift, and platform adapters? |
| [Saved Devices scenario matrix](saved-devices-scenario-matrix.md) | Which relationship, transfer, and action facts must platform read models agree on? |
| [Migration plan](migration-plan.md) | In what order can the target architecture land safely? |

## Architectural center

VniDrop is a local peer-to-peer file transfer application. Platform code owns
user interaction and access to platform file APIs; the Rust core owns transfer
identity, policy, persistence, networking, and byte streaming.

The most important invariant is:

> Platforms open files and acquire platform permissions; Rust streams the
> transfer payload. Multi-megabyte payloads do not travel through the Kotlin or
> Swift heap as the primary data path.

Durable snapshots are authoritative. Events tell a consumer that something may
have changed; they are not a second state store.

## Related specifications

These documents remain authoritative for detailed product or implementation
rules:

- [Domain vocabulary](../../CONTEXT.md)
- [Core transfer flow](../../crates/vnidrop/CORE_FLOW.md)
- [Saved Devices design](../../DESIGN-DEVICE-HISTORY.md)
- [Saved Devices platform handoff](../../DEVICE-HISTORY-UI-HANDOFF.md)
- [Repository instructions](../../AGENTS.md)

When this handbook and implementation disagree, first check whether the
statement is marked **Target**. Otherwise, treat the disagreement as
documentation drift and resolve it with the owning module and its tests.
