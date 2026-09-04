# VniDrop WinUI component system

This is the component and interaction reference for the `windows-native-ui` skill. It abstracts the repeated behavior visible in the supplied Apple screenshots and translates it into WinUI patterns. macOS chrome and exact geometry are evidence, not Windows specifications.

## Evidence boundary

The supplied screenshots show Send and Receive empty states, Send and Receive chooser sheets, a Devices list and selected-device detail, the Settings hub, and Preferences, Appearance, Storage, Network, and About drill-ins.

They do not show a confirmation dialog, destructive dialog buttons, hover or pressed visuals, light theme, high contrast, busy submission, validation failure, or narrow-window behavior. Derive those states from the user's explicit requirements and Windows control conventions; never claim they were visible in the screenshots.

## Shared foundations

- Store spacing, typography, icon sizes, surfaces, strokes, control metrics, focus, accent, warning, and critical colors in shared theme resources.
- Begin with WinUI control geometry and interaction states. A product override is shared by role rather than repeated on pages.
- Use a restrained spacing rhythm based on `4, 8, 12, 16, 24, 32`; select from it through semantic resources such as row gap, section gap, and page gutter.
- Use Mica for the persistent window backdrop and Acrylic only for appropriate transient surfaces.
- Use one Fluent icon family. Decorative icons leave the accessibility tree; actionable icons have accessible names.
- Keep accent, warning, and critical semantics separate: accent advances the task, warning explains risk, and critical marks destructive action.

## Component contracts

### `AppShell`

Own one `NavigationView`, the extended title bar, the content frame, navigation history, and width states.

- Wide: labeled navigation pane and normal page gutter.
- Medium: compact navigation and reduced gutter.
- Narrow: overlay/minimal navigation, visible pane toggle, and one content column.
- Settings lives in the footer.
- Back state comes from the frame stack and supports the built-in back button plus Alt+Left.

### `PageHeader`

Own the page title, optional description, optional back affordance, and a `CommandBar` collection. Secondary commands move to overflow before content is compressed. Root pages and drill-ins use the same height and alignment rules.

### `EmptyState`

Own a semantic icon, title, short bounded description, one primary action, and an optional secondary action. Send and Receive use this same anatomy. The page-header command may mirror the primary action while using the same action role.

### `ConnectedGroup` and `SettingsRow`

Own the grouped surface, internal separators, row padding, and row interaction states.

A row provides slots for:

- leading icon;
- title and optional description;
- optional current value or status;
- trailing control, badge, chevron, or overflow action.

The whole navigational row is one target. First and last rows inherit group corners; interior rows do not invent their own cards.

### `EntityListRow` and `TransferRow`

Use a bound `ListView` or `ItemsRepeater`. Own hover, selection, focus, keyboard activation, virtualization, ellipsis, accessible text, and a consistent More/context-menu path.

Status uses icon or text alongside semantic color. Long identifiers ellipsize predictably and expose their full accessible value without adding a casual clipboard action.

### `ListDetailsLayout`

Own explicit empty-selection, selected, loading, removed, and error states.

- Wide: list and detail pane separated structurally.
- Medium: choose list-only or a constrained details pane according to usable width.
- Narrow: selecting an item navigates to a detail page with Back.

The Devices detail groups identity, primary actions, label editing, direct transfers, and a separate critical-actions section.

### `Notice`

Use `InfoBar` with Information, Success, Warning, or Error severity. Warning orange and destructive red retain different meanings. Errors and progress changes use appropriate live-region announcements.

### `StandardDialog`

Use stock `ContentDialog` through one dialog service that guarantees one active dialog per `XamlRoot`. The service owns focus restoration, keyboard dismissal, async submission, and keeping failures visible.

Dialog variants share title, concise explanation, content slot, optional notice, and footer geometry:

- **Choice:** connected choice rows; Close is neutral dismissal.
- **Confirmation:** accent primary action and neutral Cancel.
- **Destructive confirmation:** critical-filled destructive action and neutral Cancel.
- **Progress/information:** task status with a neutral Close action when dismissal is allowed.

Size to content within the app viewport. Keep the footer visible; bound and scroll the content region only when necessary. A choice that grows into review, editing, multiple steps, or ongoing progress becomes a page or task pane.

## Action roles

Every ordinary button or action row consumes one shared role. Pages do not set local backgrounds, foregrounds, radii, or pointer states.

| Role | Rest | Hover and press | Keyboard/dialog rule |
|---|---|---|---|
| Primary | Accent fill with contrast text | Native accent states | Default for an ordinary forward decision |
| Secondary | Neutral system button | Native neutral states | Optional alternative action |
| Cancel | Neutral system button | Native neutral states | Escape invokes it; always available before work starts |
| Destructive row | Critical icon and text on a neutral surface | Critical-tinted hover, stronger pressed state | Opens confirmation when consequences require it |
| Destructive confirm | Critical fill with contrast text | Critical hover and pressed states | Cancel receives initial/default focus; Enter must not delete accidentally |
| Subtle/icon | Transparent or low-emphasis native control | Native reveal/hover state | Back, refresh, pane toggle, and overflow only |

All roles share control height, padding scale, corner-radius source, icon gap, disabled behavior, and focus-ring treatment. Use verb-specific labels such as `Delete all transfers`, `Forget device`, or `Block device`. `Cancel` dismisses a pending decision; `Close` dismisses information or a chooser with no pending commit.

While an action commits, prevent duplicate submission, preserve footer geometry, disable unavailable actions with system resources, and show progress. On failure, keep the dialog open, show an inline error/`InfoBar`, announce it, and focus a useful recovery target.

## Flow patterns

### Send and Receive

- Empty pages use `EmptyState` and one matching page command.
- A compact acquisition choice may use `StandardDialog` or a native command flyout when it needs no explanation.
- Native pickers acquire files, folders, and invitation files.
- Review, policy selection, transfer progress, and recoverable errors use navigable or in-content workflow surfaces.

### Devices

- The collection uses `EntityListRow` inside `ListDetailsLayout`.
- The selected detail provides Send files, Label, direct-transfer history, and More/context commands.
- Forget and Block live in a separated critical section and use the common destructive dialog contract.
- Removing or blocking the selected device returns focus and selection to a predictable list location.

### Settings

- The root is a hub of `ConnectedGroup` rows with icons, current summaries, and chevrons.
- Preferences, Appearance, Storage, Network, and About are real drill-in pages with shell-owned Back navigation.
- Ordinary toggles and selectors persist immediately. Apply appears only where activation is truly deferred, such as restarting network behavior.
- Storage separates maintenance from destructive history removal.
- Network uses choice rows, progressive disclosure, `InfoBar`, relay rows, and an explicit disabled/applying/error state.
- About uses grouped, readable information with a bounded content width.

## Review failures

Reject the result when any repeated task has a page-specific component, modal buttons change roles between screens, narrow layouts preserve a squeezed split view, action color carries meaning alone, a destructive hover remains neutral, a dialog grows into a multi-step workflow, page content sets one-off control metrics, or a state in the affected flow has no rendered evidence.
