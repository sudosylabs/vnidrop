# Security

## Trust model

VniDrop authenticates app installations, not human identities. An Iroh endpoint
ID proves which installation is on a connection. Authorization still depends on
the transfer, access policy, approval state, and, where applicable, the current
device-relationship generation.

The default provider posture is deny. Content becomes available only through an
explicit, scoped Invitation or Targeted transfer grant.

## Authorization boundaries

- Transport authentication answers “which endpoint is connected?”
- A device relationship answers “what durable mutual consent and directional
  grants currently exist?”
- Transfer approval answers “may this specific transfer proceed?”
- Provider authorization answers “may this peer fetch these exact content
  hashes now?”

No one answer substitutes for the others. The Targeted transfer module holds
authorization custody for its lifecycle and consults the relationship module
through a private seam.

Cancellation, abandonment, block, forget, and relationship reset must revoke
local access immediately. A reconnect or late approval cannot restore an older
grant after its generation or transfer authority has been revoked.

## Secrets and identity

Private identity material and relationship secrets live in platform credential
storage. SQLite holds opaque handles and public/non-secret state. Reset and
recovery flows reconcile both stores without logging or returning secret
material through presentation APIs.

Tickets, full endpoint IDs, and reusable authorization material are sensitive
enough to redact from production logs. User-facing diagnostics should prefer
short, non-reusable identifiers.

## File safety

The receive path treats destination names and paths as untrusted input.

- Normalize and validate relative paths; reject traversal and absolute paths.
- Keep transfer payload streaming in Rust.
- Write to temporary output, finish or abort each started file exactly once,
  and publish with no-overwrite semantics.
- Do not follow a convenience path that can silently replace an existing file.
- Preserve platform access leases for the complete operation and release them
  on every terminal path.

Android directory descriptors are not a supported Rust input. Kotlin expands a
selected SAF tree to file descriptors plus validated relative names.

## Protocol and resource limits

Handshake and Targeted protocol inputs are versioned, bounded, and validated
before they allocate large resources or mutate durable state. The provider
checks both authenticated peer identity and the requested content set.

Cancellation signals active byte work before awaiting slower cleanup, while
locks and database guards are never held across asynchronous network work.

## Security-preserving architecture rules

- Durable state, authorization, and recovery are co-owned by the same deep
  domain module.
- Wire adapters translate messages; they do not invent grants.
- UI state never becomes an access-control source.
- Network-mode policy is not silently weakened to improve connectivity.
- Events do not carry secrets or replace an authoritative read.
