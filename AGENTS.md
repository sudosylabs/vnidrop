# AGENTS.md

Operational instructions for coding agents working in this repository.
Humans: see `README.md` for product overview and run configs.
Agents: read this file (and the nearest nested `AGENTS.md`) before editing.

Nested guides take precedence when editing under those trees:

- [`crates/vnidrop/AGENTS.md`](crates/vnidrop/AGENTS.md) — Rust core
- [`shared/AGENTS.md`](shared/AGENTS.md) — Compose Multiplatform UI / KMP

---

## Project overview

VniDrop is a cross-platform **local P2P file transfer** app.

| Layer | Path | Responsibility |
|-------|------|----------------|
| Rust core | `crates/vnidrop/` | Iroh endpoint, blobs, SQLite, tickets, approval, streaming |
| Shared KMP | `shared/` | Compose UI and platform bridges for Android, Windows, and Linux |
| Compose hosts | `androidApp/`, `desktopApp/` | Thin Android and Windows/Linux app shells |
| Apple app | `apple/` | Native SwiftUI UI using generated Rust/UniFFI Swift bindings |

**Invariant:** UI/platform opens files and handles pickers; **Rust streams bytes**.
Do not design features that move transfer payloads through Kotlin heap by default.

Domain docs (reference, do not paste into PRs):

- [`crates/vnidrop/CORE_FLOW.md`](crates/vnidrop/CORE_FLOW.md)
- [`crates/vnidrop/tests/README.md`](crates/vnidrop/tests/README.md)

---

## Absolute rules

1. Prefer PRs into `master`. Do not merge to `master` locally unless the user asks.
2. Do not `git push`, force-push, or open a PR unless the user asks.
3. If `commit.gpgsign` is enabled, create **signed** commits only. If signing fails
   (empty `ssh-add -l`), stop and tell the user to unlock the key. Never switch to
   unsigned commits to “unblock” yourself.
4. Change only files required for the task. No drive-by refactors, dependency bumps,
   or repo-wide formatting.
5. Do not force architecture migrations (MVI, Hilt, Nav3, etc.) unless requested.
6. Never commit secrets, key material, or passphrases.
7. Destructive git (`reset --hard`, `push --force`, dropping DBs) only with explicit
   user approval.
8. **Every bug fix includes a regression test** at the lowest layer that catches it.
9. After code changes, run the **relevant** checks in [Build and test](#build-and-test)
   and fix failures before finishing.
10. **`localization/strings.json` is the single source of truth for all localized
    strings.** The KMP Compose resources (`shared/src/commonMain/composeResources/
    values*/strings.xml`) and the Apple catalog + accessors
    (`apple/VniDrop/Resources/Localizable.xcstrings`, `apple/VniDrop/Generated/
    L10n.swift`) are **generated** by the loc CLI (`cd localization && bun run
    src/cli.ts generate`) — never hand-edit them. To add/change a string: edit
    `strings.json` (set `targets` to `kmp`, `apple`, or omit for both), then
    regenerate. A key referenced in code but only present in a generated file will
    be silently dropped the next time generation runs.

---

## Build and test

Install prerequisites when missing: GNU Make + Bash, Rust stable + rustfmt + clippy, JDK 17,
Android NDK/SDK only if building Android, Xcode only for the native Apple app.

### Rust core (`crates/vnidrop` or workspace root)

Run from the **repo root** (Cargo workspace):

```bash
make check-rust
```

Focused:

```bash
make test-rust
make test-rust-output-sink
make test-rust-transfer
make test-rust-approval
make test-rust-lifecycle
```

After finishing Rust edits, format:

```bash
make format
```

`make check-rust` includes documentation with warnings denied, matching
`.github/workflows/rust-core.yml`.

### Shared KMP / Compose (`shared/`)

```bash
make check-shared
```

Other targets (slower / machine-dependent):

```bash
make test-android-host
make check-android
make run-desktop
```

**Note:** `jvmTest` CI runs on **Linux**. Gobley host cargo is enabled for the
current host and architecture, so local desktop builds embed their matching
Rust library.

### What to run before finishing

| You changed… | Minimum verification |
|--------------|----------------------|
| `crates/vnidrop/**` only | `make check-rust` |
| Cancel / export / sinks | Above + `make test-rust-output-sink` |
| `shared/**` only | `make test-shared` |
| Both | `make test-rust test-shared` |
| Docs only | No suite required; verify links/paths |

Do not kill long `cargo` / Gradle runs mid-flight unless they hang past several
minutes with no output; first builds are slow.

---

## Repository map (edit here)

### Rust runtime (keep split; do not re-merge into one file)

```
crates/vnidrop/src/runtime/
  mod.rs        # CoreInner, startup recovery, emit helpers
  facade.rs     # UniFFI VnidropCore + block_on
  share.rs      # import / share
  receive.rs    # receive, download, export, output sinks
  lifecycle.rs  # cancel, delete, status, access mode, shutdown
  provider.rs   # provider events, per-connection send progress
```

Other core modules: `filesystem.rs`, `invitation/`, `approval.rs`,
`handshake.rs`, `ticket.rs`, `access_policy.rs`, `event_hub.rs`, `api.rs`.

### Shared app

```
shared/src/commonMain/kotlin/com/vnidrop/app/
  core/           # CoreGateway, models, pickers interfaces
  feature/send|receive|approvals|settings|app/
  ui/theme|components|navigation|feedback|state/
androidMain|jvmMain/   # expect/actual implementations
```

### Platform file rules (do not violate)

- Windows/Linux desktop: paths; directory walk in Rust when `is_directory`.
- Android **share**: ParcelFileDescriptor **file** FDs only — never a directory FD.
  Folder share expands SAF trees in Kotlin to per-file FDs + relative names.
- Android **receive** default: MediaStore Downloads sink; custom trees via SAF write.
- Receive publish: no-overwrite temp + hard link / exclusive rename
  (see `CORE_FLOW.md`).

---

## Code style

### General

- Match surrounding code (naming, imports, error handling).
- Prefer small, reviewable diffs. Avoid files growing past ~800 LoC without
  splitting when adding substantial logic.
- Do not add one-off helpers used only once if an inline block is clearer.
- Prefer exhaustive `when` / `match`; avoid wildcards that hide new cases.

### Comments (strict)

Comment **why**, invariants, and platform/concurrency traps only.

- Do comment: cancel-before-await ordering, SAF/FD limits, security-scoped
  leases, “exactly one finish/abort after start_file”, durability rules.
- Do **not** comment: restating the next line, tutorial narration, section
  banners that repeat the function name, pasted docs from this file.

### Rust

- Follow Clippy with `-D warnings` (CI fails otherwise).
- Do not hold `std::sync::MutexGuard` or other guards across `.await`.
- Prefer `Handle::block_on` via existing `VnidropCore::block_on` for concurrent
  API entry; cancel signals active transfers **synchronously** before async work.
- Prefer private modules; export only what UniFFI / other crates need.
- New public traits/types: short docs when the role is non-obvious.

### Kotlin / Compose

For UI and presentation work, **load and follow** the in-repo skill:

```text
.codex/skills/compose-skill/SKILL.md
```

- Open at most one `references/*.md` file when the skill’s Quick Routing requires it.
- Do not invent a second Compose style guide.
- VniDrop uses **MVVM-style** ViewModels (`*State` + `StateFlow` + named methods),
  not a forced MVI `onEvent` base — adapt, do not rewrite.
- Theme via `LocalVniDropColors` / `VniDropThemeTokens` only.
  Brand primary (light): HSL `271, 91%, 65%` ≈ `#A855F7`.
- Strings: CMP `Res.string.*` / composeResources — not Android `R` in `commonMain`.
- Verify multiplatform target support before adding AndroidX/Jetpack deps to
  `commonMain`.

Details: [`shared/AGENTS.md`](shared/AGENTS.md).

---

## Testing instructions

- Prefer deterministic tests (gates, fixed sizes, public API fixtures).
- Avoid long sleeps; if polling is required: short interval + hard timeout +
  clear assertion message.
- Rust integration tests use **public** UniFFI API + `tests/support/` only.
- Failure paths: assert durable status and/or events when applicable, not only
  the error string.
- Do not add tests for pure static constants.
- Do not add negative tests for code you deleted.
- Prefer comparing whole objects when equality is meaningful.

Layout:

| Layer | Location |
|-------|----------|
| Rust unit / private | `crates/vnidrop/src/tests/` |
| Rust integration | `crates/vnidrop/tests/` |
| Shared logic | `shared/src/commonTest/` |
| Shared Compose/JVM | `shared/src/jvmTest/` |

---

## Git and PR instructions

### Branches

Name the change, not a roadmap step:

- Good: `feat/folder-share`, `fix/cancel-export-hang`, `docs/agents-md`
- Bad: `feat/step3-remaining`, `wip`, `temp`

After a PR merges: delete the feature branch **locally and on `origin`**, then
branch from updated `master`.

### Commits

- Style in history: `feat(scope):`, `fix(scope):`, `refactor(scope):`, `docs:`, `ci:`.
- Subject = outcome; body only when needed.
- Signed when repo requires it.

### Pull requests

- Title matches the main change.
- Summary: short bullets of what/why.
- **Test plan must be executable for this PR**:
  - exact commands, and/or
  - 1–2 concrete scenarios that would catch a regression.
- No filler plans (“everything works”, “CI green”) without commands or scenarios.

---

## Security considerations

- Treat tickets and endpoint IDs as sensitive enough not to log full blobs in
  production paths.
- Do not weaken approval/access checks for convenience.
- Do not store secrets in the repo; app data dirs and key files stay out of git.
- Be careful with file publish races (no-clobber rename/link policy exists for a reason).

---

## Common tasks → start files

| Task | Start here |
|------|------------|
| Share / multi-file / folders | `runtime/share.rs`, `filesystem.rs`, platform `FileSystemService.*` |
| Receive / export / sinks | `runtime/receive.rs` |
| Cancel / delete / stop share | `runtime/lifecycle.rs`, `facade.rs` |
| Per-receiver send progress | `runtime/provider.rs`, `ui/state/AppUiModels.kt` |
| Approvals | `feature/approvals/`, `approval.rs` |
| QR / NFC invitations | `TransferShareActions.*`, `ReceiveInvitationActions.*` |
| Theme / brand | `ui/theme/VniDropTheme.kt` |
| Compose skill | `.codex/skills/compose-skill/SKILL.md` |

---

## Anti-patterns (never)

- Streaming multi-MB transfer data through Kotlin as the primary design
- Passing Android **directory** FDs into Rust
- Nested exclusive `Runtime::block_on` that deadlocks cancel during receive
- Holding locks across `.await`
- Rebuilding a monolithic `runtime.rs`
- Flaky multi-minute sleeps in tests
- Unsigned commits when signing is required
- Force-push or secret commits without explicit user direction
- Hand-editing generated localization files (`values*/strings.xml`,
  `Localizable.xcstrings`, `L10n.swift`) instead of `localization/strings.json`

---

## Implementation checklist

1. Read this file + nearest nested `AGENTS.md`.
2. For Compose/UI: load `compose-skill`.
3. Smallest correct change; tests for bugs/behavior changes.
4. Run relevant build/test commands; fix failures.
5. Sparse comments only where non-obvious.
6. Commit (signed) / push / PR only as the user requests.
7. Summarize what changed and what you ran.
