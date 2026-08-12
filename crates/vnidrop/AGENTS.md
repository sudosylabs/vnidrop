# AGENTS.md — `crates/vnidrop` (Rust core)

Nearest guide when editing under `crates/vnidrop/`. Root [`AGENTS.md`](../../AGENTS.md)
still applies; this file wins for Rust-specific commands and conventions.

---

## Purpose

This crate is the transfer backend exposed to Kotlin via UniFFI (`VnidropCore`).
It owns Iroh, blobs, SQLite history, tickets, approval handshake, and file streaming.

Read [`CORE_FLOW.md`](CORE_FLOW.md) before changing send/receive/export/cancel.

---

## Commands (run from repo root)

Always prefer workspace commands so lockfile/fmt stay consistent:

```bash
make format
make test-rust
make check-rust
```

Focused integration suites:

```bash
make test-rust-transfer
make test-rust-approval
make test-rust-lifecycle
make test-rust-output-sink
```

Docs (CI uses `-D warnings`):

```bash
RUSTDOCFLAGS='-D warnings' cargo doc -p vnidrop --no-deps
```

Run `make format` after finishing Rust edits without asking.

---

## Module layout

```
src/
  runtime/
    mod.rs        # CoreInner, startup recovery, emit helpers
    facade.rs     # UniFFI surface, block_on, cancel entry
    share.rs      # share / import
    receive.rs    # receive, download, export, OutputSinkFile
    lifecycle.rs  # cancel share, delete, status, access mode, shutdown
    provider.rs   # provider messages, per-peer transfer progress
    saved_devices.rs # saved-device pairing, forget, block
    targeted.rs   # saved-device targeted transfers
  persistence.rs  # AppDataStores / persistence open (domain stores)
  invitation/     # invitation-transfer domain store (type name: Repository)
  pairing_eligibility/ # eligibility service + store
  device_relationship/ # store + service + protocol (ALPN pairing)
  targeted_transfer/   # targeted protocol + store adapter
  blocked_devices.rs
  secure_secret/  # custody + platform credential adapters (+ metadata store)
  filesystem.rs   # collect sources, atomic publish, path rules
  approval.rs / handshake.rs / ticket.rs / access_policy.rs / event_hub.rs
  api.rs          # UniFFI records/enums
  tests/          # crate-private unit tests
tests/            # public-API integration tests + support/
```

**Do not** reassemble a single huge `runtime.rs`. Prefer new focused modules if a
file approaches ~800 LoC of non-test code. Do not add new `SqlitePool` call sites —
open domain stores via `persistence::open_all`.

---

## Hard constraints

1. **Public API stability:** UniFFI surface changes break Kotlin. Prefer additive
   changes; update shared Kotlin call sites in the same change when required.
2. **Streaming stays in Rust.** Platform passes paths or FDs; core does not pull
   whole files into Kotlin.
3. **Android FDs are files only.** `SourceKind::FileDescriptor` with
   `is_directory=true` must fail; directories are expanded on the platform side.
4. **Cancel:** signal active-transfer oneshot **synchronously** before async DB
   work. Use existing `take_active_transfer` / facade cancel path. Do not reintroduce
   nested exclusive `Runtime::block_on` deadlocks.
5. **SecureSecretStore:** never call the sync store from an async task body.
   Linux Secret Service / zbus blocking nests Tokio `block_on`; `SecretCustody`
   must keep those calls on `spawn_blocking`.
6. **No lock across await:** Clippy `await_holding_lock` fails CI.
7. **ReceiveOutputSink:** after successful `start_file`, exactly one of
   `finish_file` or `abort_file` (see `OutputSinkFile` Drop).
8. **No-overwrite publish** for path receives (temp + hard link / exclusive rename).
9. Integration tests must use the **public** API + `tests/support/` only.

---

## Code style (Rust)

- Clippy clean with `-D warnings`.
- Prefer exhaustive `match`; avoid catch-all arms that hide new enum variants.
- Prefer comparing whole objects in tests when practical.
- Comment only non-obvious why (concurrency, durability, platform FS quirks).
- Do not add one-off private helpers used once if inline is clearer.
- Prefer private modules; export deliberately via `lib.rs` / UniFFI.

---

## Testing

- Unit / private: `src/tests/` (see crate `tests.rs` paths).
- Integration: `tests/*.rs` + `tests/support/mod.rs` (`TestNode`, `MemoryOutputSink`,
  `CoreGuard`, etc.).
- Bug fixes need a regression test.
- Prefer gates/latches over multi-second sleeps (see output_sink cancel test).
- Failure tests: durable status and/or events when applicable.
- Recovery tests: shut down core, reopen same data dir.

Details: [`tests/README.md`](tests/README.md).

---

## PR / verify checklist for this crate

```bash
make check-rust
```

If you touched cancel, export, or sinks, also:

```bash
make test-rust-output-sink
```
