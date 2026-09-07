---
name: windows-native-ui
description: Coherent native Windows UI for VniDrop under windows/. Use for WinUI shell or navigation, reusable components, dialogs and action semantics, responsive layouts, Devices or Settings flows, Send or Receive flows, and rendered visual QA.
---

# Coherent VniDrop UI for Windows

**Coherence** means that the same task uses the same component and state contract everywhere. Preserve VniDrop's capabilities while choosing Windows-native structure, controls, navigation, and interaction behavior.

## Workflow

### 1. Account for the experience

Inventory the affected flow before editing:

- Use Apple and KMP screens as evidence for capabilities, hierarchy, states, and product tone.
- Use Windows Settings, File Explorer, WinUI Gallery, and Microsoft guidance for Windows presentation and behavior.
- Inspect the current WinUI implementation for existing state, core calls, and platform integration.

This step is complete when every visible action, state transition, error, and secondary surface in the affected flow is listed, including empty, loading, disabled, selected, confirmation, success, and failure states.

### 2. Select the native pattern

Read [references/component-system.md](references/component-system.md) when the change touches a reusable component, dialog, action, collection row, empty state, Settings, Devices, Send, or Receive.

Choose the stock WinUI control or established shared component that owns the behavior. Add or deepen a shared component when pages would otherwise repeat styling, state logic, or accessibility behavior.

This step is complete when every surface maps to one component contract and no page needs a private copy of a shared button, row, modal, or responsive rule.

### 3. Build from the shell inward

For broad work, use this order:

1. Adaptive `NavigationView`, title bar, and real `Frame` navigation history.
2. Semantic resources and shared action, row, dialog, header, and empty-state components.
3. Devices list/detail and Settings hub/drill-in patterns.
4. Send and Receive acquisition, review, progress, and completion flows.
5. Remaining details and secondary surfaces.

Pages consume shared roles; they do not set local colors, corner radii, control heights, or hover behavior. Full workflows use navigation or an in-content task surface. `ContentDialog` handles compact choices and decisions.

This step is complete when shell and shared-component changes are rendered successfully before page-specific polish begins.

### 4. Prove the interaction states

Capture the changed surface at approximately 1200, 800, and 500 effective pixels. Verify light, dark, and high-contrast themes when color or materials change. Exercise mouse, keyboard, and context-menu paths.

For every changed interactive component, inspect rest, hover, pressed, focus, disabled, selected, busy, validation, and error states that it supports. Check long localized strings, text scaling, and the narrowest supported window.

This step is complete when all affected states are reachable, no action or content requires horizontal scrolling, one component role looks and behaves the same on every page, and the captures show real reflow rather than compression.

## Product constraints

- Windows file and folder acquisition uses native pickers.
- Invitation sharing uses the Windows share contract. Ticket clipboard and copy/paste actions stay absent unless the user explicitly requests them.
- Transfer payloads remain in the Rust streaming path; managed UI code selects sources and destinations.
- Status always has text or an icon in addition to color.
- The application keeps one vertical scroll owner per page and one active dialog per `XamlRoot`.

## Completion gate

A UI change is complete only when:

- each affected flow state is accounted for;
- shared component contracts cover every repeated surface;
- Windows navigation, focus, keyboard, and accessibility behavior work;
- destructive, warning, accent, neutral, and disabled meanings remain distinct;
- wide, medium, and narrow captures have been inspected;
- the result feels native beside current Windows first-party apps without copying their pixels.
