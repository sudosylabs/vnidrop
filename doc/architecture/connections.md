# Connections

## Identity and reachability

An Iroh endpoint ID identifies and cryptographically authenticates an app
installation. It is not a user account, a device name, an IP address, or proof
that the peer is currently reachable.

An endpoint address describes ways to reach that identity. It may contain direct
addresses and relay URLs. Discovery and relay selection can change without
changing the endpoint identity or a saved-device relationship.

The application distinguishes:

- **Connection:** a live authenticated network session.
- **Device relationship:** durable mutual consent and directional grants for two
  endpoint identities at a relationship generation.
- **Saved device:** the presentation of that relationship to a user.

A network connection does not itself grant file access.

## Router protocols

The Rust core owns one Iroh endpoint and registers protocol handlers by ALPN.
The important handlers are:

| ALPN | Responsibility |
|---|---|
| `iroh_blobs::ALPN` | Authorized blob transfer |
| `/vnidrop/handshake/2` | Invitation metadata, approval, and receipt handshake |
| Relationship protocol | Pairing and relationship-generation operations |
| Targeted transfer protocol | Offer, consent, lifecycle coordination, and targeted authorization |

Protocol handlers translate authenticated wire messages into domain operations.
They do not become independent lifecycle authorities.

## Network modes

VniDrop supports these routing policies:

| Mode | Relay behavior | Direct behavior |
|---|---|---|
| Automatic | Uses the configured/default relay facilities | Uses direct discovery when available |
| Strict custom | Uses only the configured custom relays | No direct fallback that violates the policy |
| Custom with direct fallback | Prefers configured custom relays | May connect directly when possible |
| Local only | Does not use internet relays | Restricts discovery and transport to local reachability |

Relay compatibility is enforced when tickets or peer addresses are interpreted.
The application must not silently broaden a strict routing policy to make a
transfer succeed.

## Relays and discovery

Relays help two authenticated endpoints establish encrypted connectivity when a
direct route is unavailable. They forward encrypted traffic; they are not a
VniDrop file store, transfer database, user account system, or push/wakeup
service.

Because there is no guaranteed remote wakeup, an offer can remain passive until
the receiving app is available. Sender content availability during an active
negotiation or transfer is represented as a Runtime obligation rather than
being inferred from a socket alone.

## Connection lifecycle rules

- Authenticate the endpoint before interpreting application messages.
- Apply access policy and relationship grants independently of transport
  success.
- Do not treat reconnect as a new transfer attempt; durable transfer identity
  controls recovery.
- Do not log complete tickets, endpoint IDs, or other reusable connection
  material in production paths.
