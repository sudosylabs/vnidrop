# Iroh

## Role in VniDrop

Iroh provides the authenticated endpoint, QUIC-based connectivity, protocol
routing, content addressing, and blob transfer used by the Rust core. VniDrop
adds the product-level concepts Iroh intentionally does not define: transfer
identity, approval, device relationships, access grants, history, and platform
presentation.

The boundary is deliberate:

| Iroh provides | VniDrop provides |
|---|---|
| Endpoint identity and authenticated connections | Installation identity semantics and saved-device relationships |
| Direct/relay reachability | User-selected network policy |
| Content-addressed blob import and fetch | Transfer manifests, names, lifecycle, and history |
| `BlobTicket` transport information | Wrapped invitation tickets, approval, and access rules |
| Protocol router | Handshake, relationship, and Targeted transfer domain protocols |

## Endpoint and router

The core constructs one long-lived Iroh endpoint and router. Application
protocols share that endpoint so they agree on installation identity and network
configuration. Platform layers do not create parallel endpoints.

Startup wiring belongs in the runtime root; protocol-specific behavior belongs
behind its owning domain module. Shutdown must stop accepting new work, settle
active operations, and close shared Iroh resources in a defined order.

## Blob store and manifests

Share sources are imported into the Iroh blob store. Content hashes become part
of the durable transfer identity or manifest, allowing the sender to serve
content without copying it through a platform UI process buffer.

A successful import is not by itself a Targeted transfer. In the selected target
design, import and hashing before durable registration are **Transfer
preparation**. The deep Targeted transfer module commits immutable identity and
lifecycle state at registration.

The provider must authorize every requested hash against the relevant
Invitation or Targeted transfer grant. Possessing a hash or reaching the Iroh
blob protocol does not imply permission.

## Tickets

VniDrop invitation tickets use the `vnd1:` envelope. The envelope carries
application metadata and backup relay URLs around Iroh blob addressing. Parsing
validates the VniDrop format and rejects raw `BlobTicket` input so callers cannot
bypass application handshake and policy.

Tickets are bearer-like connection material and should be shown, copied, and
logged only where the product explicitly requires it.

## Recovery and availability

Iroh resources are runtime capabilities; SQLite transfer records are durable
truth. On startup, the core reconciles persisted lifecycle state with available
content and reconstructs the provider authorization needed for recoverable work.

Iroh activity is one input to Runtime obligation facts, but socket counts are not
the policy. Preparation, negotiation, and provider availability can require
process retention even when no byte stream is active at that instant.

## Code map

- Runtime construction and shared resources:
  [`crates/vnidrop/src/runtime/`](../../crates/vnidrop/src/runtime/)
- Invitation handshake:
  [`crates/vnidrop/src/handshake.rs`](../../crates/vnidrop/src/handshake.rs)
- Ticket envelope:
  [`crates/vnidrop/src/ticket.rs`](../../crates/vnidrop/src/ticket.rs)
- Filesystem and source import rules:
  [`crates/vnidrop/src/filesystem.rs`](../../crates/vnidrop/src/filesystem.rs)
- Detailed flow:
  [`crates/vnidrop/CORE_FLOW.md`](../../crates/vnidrop/CORE_FLOW.md)
