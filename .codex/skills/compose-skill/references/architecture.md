# VniDrop presentation architecture

Use for feature structure, state ownership, deepening, or ViewModel/UI seams.

## Module shape

```text
Route
  ├─ collects StateFlow
  ├─ invokes platform adapters
  ├─ collects semantic effects
  └─ navigates
       ↓
Deep presentation module
  ├─ immutable state
  ├─ named methods
  ├─ domain orchestration
  └─ internal seams
       ↓
CoreGateway + platform adapters
```

| Concern | Owner |
|---|---|
| Domain-facing presentation state | Feature ViewModel/module |
| Platform picker invocation | Route |
| Navigation | Route/app navigation |
| Transfer creation and durable state | Rust through `CoreGateway` |
| Source preparation | Android/JVM adapter |
| Rendering | Screen and leaf composables |
| One-shot feedback | Existing effect/`UiMessageController` pattern |

## Depth checks

- Apply the deletion test: deleting a deep module must spread meaningful behavior back across multiple callers.
- The interface is the test surface. If tests need private state, reshape the module.
- Keep adapters internal unless callers genuinely select them.
- Prefer a concrete class when only one implementation exists.
- Do not create an `Actions` bag to conceal a wide interface.
- Do not split by arbitrary file size when the pieces still share one interface and invariant set.

## State rules

- Make states immutable and equality-friendly.
- Store durable/product-significant state in the module; keep focus, scroll, animation, and transient expansion local to Compose.
- Derive values instead of storing duplicates.
- Track provenance when derivation must stop after user editing, such as automatic Transfer draft names.
- Model loading without discarding valid content.
- Freeze invalid concurrent operations explicitly in state.

## Transfer draft seam

The external seam is a session-scoped MVVM module used by both callers.

```kotlin
class TransferDraftViewModel(...) : ViewModel() {
    val state: StateFlow<TransferDraftState>
    val outputs: Flow<TransferDraftOutput>

    fun openInvitation(defaultSenderName: String)
    fun openTargeted(device: LockedSavedDevice)
    fun chooseFiles()
    fun chooseFolder()
    fun onPickerResult(requestId: Long, result: Result<List<PickedShareFile>>)
    fun changeTransferName(value: String)
    fun removeSource(id: DraftSourceId)
    fun submit()
    fun dismiss()
}
```

The exact implementation may evolve, but preserve these invariants:

- One immutable destination per open session.
- One in-flight picker and submission.
- Correlate picker callbacks; discard stale owned copies.
- Atomic source replacement.
- Failure preserves the draft.
- Success/dismissal releases owned copies exactly once.
- Semantic creation output; no navigation inside the module.
- Invitation and Targeted results remain distinct.

## Migration order

1. Extract Invitation composition and interface-level tests.
2. Switch Targeted creation to the same module.
3. Delete the one-shot Saved-device picker flow.
4. Promote Saved devices into its own route.
5. Add Targeted transfer detail and lifecycle presentation.
6. Remove the experimental UI gate atomically after parity.

## Anti-patterns

| Anti-pattern | Better replacement |
|---|---|
| Parent ViewModels duplicate source rules | One Transfer draft module |
| `FileSystemService` creates transfers | Source adapter prepares; module calls `CoreGateway` |
| Generic transfer type erases domain | Explicit Invitation/Targeted variants |
| Route contains state machine | Route binds adapters and navigation only |
| Tests mock internals | Test through module state/methods/outputs |
