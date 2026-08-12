---
name: compose-skill
description: VniDrop-specific Compose Multiplatform UI and Kotlin presentation architecture. Use when designing, implementing, refactoring, or reviewing code under shared/ for Android, Windows, or Linux: screens, ViewModels, routes, navigation, adaptive layouts, platform adapters, icons, resources, accessibility, and UI tests.
---

# VniDrop KMP UI

Build VniDrop's Android and desktop UI without weakening its domain model, platform identity, or Rust streaming invariants.

## Start here

1. Read the root `AGENTS.md` and `shared/AGENTS.md` completely.
2. Read `CONTEXT.md` and only the ADRs relevant to the feature.
3. Inspect the nearby feature, its tests, and its Android/JVM adapters before designing.
4. Identify the module, interface, seam, and adapters. Prefer a deep module: small interface, substantial hidden behavior, one test surface.
5. Model state and platform behavior before drawing pixels.
6. Implement the smallest complete product flow; add regressions at the lowest useful layer.
7. Run `make test-shared`; run `make check-shared` for a production UI handoff.

## Scope and ownership

- `shared/commonMain` owns shared domain-facing presentation state, feature behavior, semantic UI structure, and reusable visual primitives.
- `androidMain` owns Android pickers, SAF, MediaStore, system surfaces, and Android-native presentation where needed.
- `jvmMain` owns Windows/Linux filesystem, desktop integration, and platform-native presentation where needed.
- Apple uses the native SwiftUI app under `apple/`. Do not move Apple presentation into KMP.
- Rust owns transfer payload streaming, authorization, durable lifecycle, and transfer persistence. Kotlin must not become the payload path.

## Architecture

Use VniDrop's MVVM-style modules:

- Immutable `*State` exposed through `StateFlow`.
- Named ViewModel methods for user actions. Do not introduce a generic `onEvent` hierarchy.
- Route: obtain/collect state, invoke platform adapters, collect effects, and perform navigation.
- Screen: render state and emit explicit callbacks.
- Leaf composables: accept narrow state and callbacks; retain only visual-local state such as focus, scroll, or animation.
- `AppGraph` wires dependencies. Do not introduce Hilt, Koin, or a second graph.

Design for depth and locality:

- Put behavior behind a small interface used by callers and tests.
- Keep internal seams private. Do not add an interface until behavior genuinely varies.
- An Android/JVM/test adapter set is a real seam; a single implementation is not.
- Do not hide callback explosion in an `Actions` data class. That changes syntax, not depth.
- Do not add use-case classes or pass-through repositories around `CoreGateway`.
- Preserve `Invitation transfer`, `Targeted transfer`, `Transfer draft`, `Saved device`, and `Device relationship` as distinct terms from `CONTEXT.md`.

For structural work, read [references/architecture.md](references/architecture.md). Open no other reference in the same turn unless the task changes materially.

## Platform-native experience

Every supported platform should feel native. Sharing implementation is a means, not the goal.

- Prefer shared behavior and state, but allow repeated Android, Windows, and Linux presentation implementations when native interaction, layout, menus, dialogs, shortcuts, density, or system integration differ.
- Do not force the lowest-common-denominator UI merely to maximize `commonMain` code.
- Keep duplicated platform presentation thin and semantic; do not duplicate domain rules or transfer state machines.
- Use `expect`/`actual`, platform source sets, or injected adapters only at genuine seams.
- Android should follow Material interaction and navigation conventions.
- Windows should use Fluent iconography and desktop interaction conventions.
- Linux/Desktop should use the existing Lucide family and desktop conventions.

### Native icons

- Use semantic `AppIcon` values rendered through `PlatformIcon`.
- Android resolves Material icons, Windows resolves Fluent icons, and Linux/Desktop resolves Lucide icons.
- When adding an icon, provide the appropriate resource for every supported family. Do not reuse one platform's asset everywhere because it is convenient.
- Prefer the native system icon or platform icon family when a platform exposes a stronger convention. A platform-specific implementation is acceptable.
- Give actionable icons a localized content description; decorative icons use `null`.
- Do not inline arbitrary Material icons or hard-code drawable selection in feature composables.

For platform-specific UI decisions, read [references/platform-native-ui.md](references/platform-native-ui.md). Open no other reference in the same turn unless the task changes materially.

## VniDrop transfer invariants

- Invitation transfer and Targeted transfer may share a Transfer draft implementation, but never erase their distinct destination, authorization, lifecycle, or result types.
- Public Targeted operations remain transfer-ID-only. Never expose authorization material to Kotlin.
- A saved-device display name resolves as local label, then authenticated remote display name, then a localized unnamed fallback. Endpoint ID is secondary diagnostic identity.
- Android folder sharing expands a SAF tree into file descriptors plus safe relative names. Never pass an Android directory FD to Rust.
- Desktop may pass filesystem directories marked as directories for Rust traversal.
- Platform source adapters keep descriptors and leases alive for the complete core call and close them exactly once.
- App-owned picker copies are released on replacement, removal, explicit dismissal, or successful creation. Never delete original user sources.
- Picker cancellation and creation failure preserve the current valid Transfer draft.

### Transfer draft architecture

Use one deep, session-scoped composition module for Invitation and Targeted creation:

- Concrete MVVM module with `TransferDraftState`, named methods, and semantic outputs.
- Domain-specific `openInvitation` and `openTargeted`; Targeted receiver is locked for the session.
- Routes invoke file/folder picker adapters and navigate from semantic creation results.
- The module owns selection, opaque source IDs, automatic-name provenance, validation, retry, single-flight submission, destination revalidation, and temporary-copy lifecycle.
- One private platform source-adapter seam serves Android, JVM, and tests.
- Multiple files or one folder; do not add mixed file-plus-folder drafts without an explicit product decision.
- Targeted mode omits Invitation sender-name and access-policy controls.
- Operational failure preserves the draft; successful creation emits the correct domain identity for the host to open.

## UI system

- Use `LocalVniDropColors` and `VniDropThemeTokens`; do not hard-code product colors.
- Use existing `WindowClass`, `LocalUiPlatform`, `contentWindowClassFor`, and shell/navigation helpers.
- Prefer semantic feature modules over generic visual abstractions.
- Keep stable keys for device and transfer lists.
- Preserve minimum touch targets, keyboard access, focus order, readable contrast, and meaningful semantics.
- Treat phone, tablet/rail, Windows desktop, and Linux desktop as deliberate presentations—not scaled copies.

## Strings and resources

- `localization/strings.json` is the only source of truth for product strings.
- Run the localization generator after editing it.
- Never hand-edit generated Compose XML, Apple catalogs, or accessors.
- Use `Res.string.*` in `commonMain`; never Android `R` there.
- Do not synthesize English product copy in ViewModels, including automatic transfer names.
- Resolve semantic strings near presentation or inject a small formatter when behavior requires localized text.

## Dependencies

- Prefer existing dependencies and platform facilities.
- Before adding AndroidX/Jetpack to `commonMain`, verify coordinates, exact API shape, and every required KMP target using official documentation or artifact metadata.
- If verification is unavailable, stop and report the uncertainty. Do not add an unverified production dependency with a “check later” comment.
- Do not add navigation, DI, persistence, networking, or image-loading frameworks unless the requested feature proves the need.

## Testing

- `commonTest`: state machines and feature behavior through the module interface; use focused fakes.
- `jvmTest`: Compose interaction, semantics, keyboard behavior, and adaptive phone/desktop presentation.
- Platform tests: Android SAF/MediaStore and desktop filesystem/native integration.
- Test complete states where relevant: empty, loading, content, busy, error, confirmation, interruption, and terminal outcomes.
- Test both Invitation and Targeted modes through the shared Transfer draft interface.
- Assert native icon-family selection and accessibility semantics when adding platform actions.
- Prefer deterministic gates and virtual time; avoid fixed sleeps.
- Delete obsolete shallow tests after equivalent interface-level coverage exists.

## Anti-patterns

- Business rules, core calls, ticket parsing, or filesystem work in composables.
- A global mutable draft shared by Send and Saved devices.
- Payload bytes streamed through Kotlin.
- Android directory FDs.
- Raw endpoint IDs as primary saved-device names.
- Generic `onEvent`, forced MVI, Hilt/Koin migrations, or use-case-per-method architecture.
- One universal icon set or one platform's interaction model imposed on every platform.
- Duplicated domain behavior justified as “native UI.” Only presentation duplication is acceptable.
- Hand-edited generated localization outputs.
- New generic wrappers whose deletion merely moves calls to the caller.
