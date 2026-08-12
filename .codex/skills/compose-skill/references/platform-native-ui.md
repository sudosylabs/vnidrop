# Platform-native UI

Use when Android, Windows, and Linux should intentionally render or behave differently.

## Native-first rule

Share domain behavior and semantic state. Duplicate presentation when sharing would make a platform feel foreign.

| Platform | Default visual language | Typical native differences |
|---|---|---|
| Android | Material + Material icons | bottom navigation, sheets, system pickers, back behavior, touch density |
| Windows | Fluent icons + desktop conventions | sidebar/command placement, context menus, keyboard shortcuts, window chrome |
| Linux | Lucide + desktop conventions | desktop menus, filesystem flows, window integration |

Acceptable duplication:

```text
commonMain: SavedDeviceState + named actions + semantic models
androidMain: AndroidSavedDeviceScreen
jvmMain: WindowsSavedDeviceScreen / LinuxSavedDeviceScreen
```

Unacceptable duplication:

```text
androidMain/jvmMain each reimplement pairing, transfer validation,
source cleanup, lifecycle mapping, or CoreGateway orchestration
```

## Choosing the seam

Use platform-specific implementation when at least one differs materially:

- Interaction convention or navigation placement.
- System picker, menu, dialog, notification, or window integration.
- Keyboard/mouse versus touch behavior.
- Icon family or system-provided symbol.
- Accessibility semantics required by the host platform.
- Layout density and information hierarchy.

Keep a shared composable when only spacing or a token changes and the interaction model remains native on every platform.

## Icons

- Add a semantic `AppIcon` case, not a feature-local drawable choice.
- Supply Material, Fluent, and Lucide resources.
- Render with `PlatformIcon` so `LocalUiPlatform` selects the family.
- Use a native system icon through a platform implementation when it communicates better than the bundled family.
- Localize content descriptions for actions. Decorative icons use `null`.
- Test `resourceFor`/family selection and important semantics.

## Adaptive layout

- Use existing `WindowClass`, platform helpers, and app shell before inventing breakpoints.
- Phone flow may be full-screen or sheet-based.
- Desktop may use persistent navigation, side panels, dialogs, context menus, and denser information.
- Do not merely enlarge phone controls on desktop.
- Do not compress desktop controls into touch-hostile phone layouts.

## Review questions

- Does this screen look and behave expectedly on each platform?
- Did sharing code force a non-native interaction?
- Is duplicated code presentation-only?
- Are domain rules still local to one shared module?
- Does every actionable icon have the correct family and semantics?
