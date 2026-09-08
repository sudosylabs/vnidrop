# Core test organization

VniDrop uses two complementary Rust test layers.

## Internal tests

Tests under `src/tests/` can exercise crate-private invariants without widening
the production API:

- `access_policy.rs`: authorization and approval-session rules.
- `filesystem.rs`: source collection and path validation.
- `handshake.rs`: malformed protocol response handling.
- `limits.rs`: core-limit validation.
- `repository.rs`: schema, persistence, and transition invariants.
- `runtime.rs`: public error mapping and orchestration (mirrors `src/runtime/`).
- `secure_secret.rs`: protected identity custody, recovery, and reconciliation.
- `secure_secret_android.rs`, `secure_secret_apple.rs`,
  `secure_secret_linux.rs`, and `secure_secret_windows.rs`: platform credential
  storage adapters.
- `ticket.rs`: ticket encoding, parsing, and metadata validation.
- `transfer_state.rs`: persisted enum parsing and legal lifecycle transitions.

## Integration tests

Files directly under `tests/` are black-box scenarios. They must use the public
`vnidrop` API and should be organized by behavior rather than implementation
module:

- `approval.rs`: receiver authorization flows.
- `lifecycle.rs`: stop, restart, recovery, and revocation behavior.
- `output_sink.rs`: foreign output-sink contracts and failures.
- `transfer.rs`: end-to-end file and directory transfers.

Reusable fixtures live in `tests/support/`. `CoreGuard` shuts down a test core
on drop, while `RecordingSink` and `MemoryOutputSink` keep assertions focused
on externally observable behavior.

## Test requirements

- Every bug fix must include a regression test.
- Avoid arbitrary sleeps. When polling an asynchronous boundary is unavoidable,
  use a short interval and a bounded timeout with a useful failure message.
- Prefer deterministic IDs, inputs, and clocks.
- Do not expose production internals solely for integration tests.
- Failure tests must verify durable status and emitted events when applicable,
  not only the returned error.
- Recovery tests must close the original core and reopen the same data directory.
