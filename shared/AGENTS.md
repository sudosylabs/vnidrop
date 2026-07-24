# AGENTS.md — `shared/` (Compose Multiplatform + KMP)

Nearest guide when editing under `shared/`. Root [`AGENTS.md`](../AGENTS.md)
still applies; this file wins for UI/KMP work.

---

## Purpose

`shared` is the Compose Multiplatform app layer for Android, Windows, and Linux:
Compose UI, feature ViewModels, and `expect`/`actual` platform bridges. Native
transfer work goes through UniFFI `VnidropCore` (see `crates/vnidrop`). Apple
platforms use the native SwiftUI app under `apple/`.

---

## Compose skill (required for UI work)

For screens, components, theme, navigation, resources, ViewModel↔UI wiring,
lists, animation, accessibility:

1. Load [`.codex/skills/compose-skill/SKILL.md`](../.codex/skills/compose-skill/SKILL.md).
2. Follow its workflow and defaults.
3. Open **at most one** file under `.codex/skills/compose-skill/references/` when
   the skill’s Quick Routing table says you need deeper guidance.
4. Do **not** invent a parallel Compose style guide.

### Project policy (overrides generic skill defaults)

| Topic | Do this |
|-------|---------|
| Architecture | Keep **MVVM-style** ViewModels: immutable `*State`, `StateFlow`, **named methods**. Do not force MVI `onEvent` sealed hierarchies unless asked. |
| Structure | Feature packages under `com.vnidrop.app.feature.*`; thin route/wiring + screen/composables. |
| Theme | Only `LocalVniDropColors` / `VniDropThemeTokens` (`ui/theme/VniDropTheme.kt`). Brand primary light ≈ `#A855F7` (HSL 271, 91%, 65%). |
| Strings | CMP composeResources / `Res.string.*` — not Android `R` in `commonMain`. `values*/strings.xml` are **generated** from `localization/strings.json` (source of truth) via the loc CLI — add/edit keys there, never in the XML. |
| DI | Follow existing `AppGraph` construction; no unprompted Hilt/Koin migration. |
| Platform | `androidMain` / `jvmMain` for pickers, SAF, NFC/QR, and desktop integration. |
| Dependencies | Before adding Jetpack/AndroidX to `commonMain`, verify multiplatform artifacts for all targets. |

compose-skill “Existing Project Policy”: adapt to this repo; do not force-migrate.

---

## Commands

From repo root:

```bash
make check-shared
```

Optional:

```bash
make test-android-host
make run-desktop
make check-android
```

CI `:shared:jvmTest` runs on **Linux**. Gobley host cargo follows the current
host and architecture so local desktop builds embed their matching Rust library.

When Kotlin changes touch UniFFI-generated APIs, rebuild/test with a full
`jvmTest` so Gobley/native pieces stay aligned.

---

## Layout

```
src/
  commonMain/kotlin/com/vnidrop/app/
    core/                 # models, CoreGateway, FilePicker interfaces
    feature/send|receive|approvals|settings|app/
    ui/                   # theme, components, navigation, feedback, state helpers
  commonMain/composeResources/
  androidMain|jvmMain/
  commonTest|jvmTest|...
```

### Platform file bridging (must preserve)

- **Android share:** open content URIs as FDs; expand **folder trees** to per-file
  documents with relative `displayName` paths before calling Rust
  (`FileSystemService.android.kt` / `expandShareDirectory`).
- **Android receive:** MediaStore Downloads sink and/or SAF tree write sink.
- **Apple:** lives outside this module under `apple/`; do not add Apple platform
  behavior back to KMP.
- **Desktop:** filesystem paths; directories may be marked `isDirectory` for Rust walk.

Never pass a directory as a single Android FD into `SourceKind.FILE_DESCRIPTOR`.

---

## Code style (Kotlin)

- Match existing feature style (imports, naming, state updates via `update { }`).
- Composables render state and invoke callbacks; no business rules in `@Composable`
  bodies (network, share creation, ticket parse — ViewModel/core).
- Prefer stable list keys from domain IDs.
- Comments only for non-obvious platform or concurrency reasons.
- Do not hard-code brand colors; use theme tokens.

---

## Testing

- Logic: `src/commonTest/` (e.g. ViewModel fakes, `AppUiModels` progress helpers).
- Compose/JVM: `src/jvmTest/`.
- Add/adjust tests when changing state machines, progress aggregation, or
  share/receive eligibility.
- Prefer fakes in `commonTest` support over real UniFFI in pure unit tests.

```bash
make test-shared
```

---

## Anti-patterns

- Streaming transfer bytes through Kotlin as the main design
- Directory FDs on Android
- New DI framework “because best practice”
- Loading every compose-skill reference file for a small UI tweak
- Android `R.string` in `commonMain`
