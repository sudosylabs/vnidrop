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

## Navigation and empty states

- Android bottom navigation keeps label color and weight stable across selection. Emphasize the selected icon with a rounded, low-opacity solid brand indicator rather than a gradient.
- Treat the Saved Devices empty state as VniDrop's density reference: a small muted decorative icon, `titleMedium` hierarchy, `bodyMedium` supporting copy, and accent reserved for the action.
- Reuse one semantic empty-state primitive when icon scale, title, description, and spacing should match. Keep screen-specific layout and actions at the caller.
- Empty-state creation and acquisition actions use a leading Add icon when they introduce a new transfer flow.

## Surfaces and actions

- Distinguish modal dialogs from drawers and bottom sheets. A dismissible dialog may own an explicit close action; a bottom sheet or drawer uses its native drag, outside-click, back, or escape dismissal without a redundant close icon.
- Decision-blocking approval dialogs expose their required accept/decline actions instead of implying that dismissal is valid.
- Destructive confirmations use quiet native dialog actions with destructive text emphasis. Reserve saturated filled destructive buttons for primary in-page destructive actions.
- Windows and Linux collection rows expose secondary actions through right-click context menus. Keep touch affordances such as overflow buttons on Android; do not impose the mobile overflow control on desktop rows.
- Preserve keyboard and accessibility access to every context-menu action even when its visible desktop affordance is right-click.

## Settings

- Use quiet platform lists, whitespace, and dividers for ordinary settings. Tinted icon tiles, bordered rounded cards, and strong selection fills require a real grouping or status reason.
- On Android, place a setting's current value below its title as supporting text. On desktop, a trailing value may remain inline only when it has a bounded share of the row and truncates before the title.
- Long usernames, translated modes, and diagnostic values must leave the setting title readable. Include these states in rendered QA.

## Review questions

- Does this screen look and behave expectedly on each platform?
- Did sharing code force a non-native interaction?
- Is duplicated code presentation-only?
- Are domain rules still local to one shared module?
- Does every actionable icon have the correct family and semantics?
- Do long values truncate or wrap without compressing their labels?
- Does each dialog, sheet, drawer, overflow button, and context menu match the host platform's interaction convention?
