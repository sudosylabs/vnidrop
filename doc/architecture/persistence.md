# Persistence

## Durable authority

SQLite snapshots are the authoritative record of relationships, transfers, and
lifecycle state. Events are wake-ups: a consumer responds to an event by reading
the relevant snapshot rather than maintaining a competing event-derived store.

The Rust core opens one persistence assembly through `persistence::open_all` and
passes focused stores to their owning modules. Sharing one database pool does not
make one repository responsible for every schema.

## Domain stores

`AppDataStores` assembles stores for these domains:

| Store area | Durable responsibility |
|---|---|
| Invitation transfers | Created shares, receive/send lifecycle, and invitation history |
| Targeted transfers | Immutable participants and manifest, lifecycle, consent results, and recoverable state |
| Device relationships | Relationship generation, directional grants, and relationship lifecycle |
| Eligibility | Policy facts used to decide whether relationship or transfer operations are allowed |
| Secrets metadata | Opaque references and reconciliation state for platform credential storage |
| Blocked identities | Local deny decisions that must survive restart |
| Identity recovery | State needed to restore or intentionally reset installation identity |

Each domain store owns its migrations and query invariants. Cross-domain
orchestration belongs in a domain module or explicit transaction, not in UI
code.

## Secrets

Secret values are stored in platform credential storage. SQLite stores opaque
handles and non-secret metadata. Operations spanning both systems use staged
reconciliation because a database transaction cannot atomically commit a
Keychain, Keystore, or desktop credential-store mutation.

Recovery must tolerate a process stopping between stages without exposing a
secret in SQLite or leaving a durable relationship that cannot authenticate.

## Transfer durability

A transfer becomes part of history at durable registration. Registration must
commit the immutable identity required to interpret all later transitions.

For the selected Targeted transfer design:

- preparation/import before registration is ephemeral;
- registration commits sender, receiver, manifest, and content identity;
- negotiation continues after registration under core ownership;
- cancellation and failure remain durable history;
- pre-approval abandonment removes the registered row only when abandon wins
  the linearized race;
- retry creates a new identity rather than rewriting old history.

State transition and corresponding authorization changes must be ordered so
that restart cannot revive access that was already revoked.

## Startup recovery

On startup, the core inspects nonterminal durable records, reconciles external
resources, and either resumes supported work or moves it to an explicit durable
outcome. It must not infer truth from the last event a platform happened to
observe.

Recovery is owned by the same deep module that owns normal lifecycle
transitions. This keeps crash behavior and live behavior subject to one set of
rules.

## Read models

**Target:** Kotlin and Swift each build an in-process Saved Devices
read model from the core's durable APIs. The implementations remain
platform-specific but share a canonical scenario matrix and domain vocabulary.

Stable UI action facts belong in those read-model modules, not scattered across
composables or views. A revisioned aggregate Rust snapshot is deferred until
there is evidence that separate reads cause user-visible torn state that cannot
be solved within the platform read model.
