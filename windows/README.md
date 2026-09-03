# Native Windows app

This is the C# / WinUI 3 / XAML implementation of VniDrop for Windows. It uses
Windows navigation, command bars, lists, dialogs, Mica, system themes, keyboard
interaction, and native file pickers. The presentation follows Windows conventions
rather than reproducing the Compose layout.

The UI calls the existing Rust core through generated UniFFI C# bindings. Rust
owns identities, approvals, pairing, history, and file streaming. C# passes file
paths; transfer payloads never pass through the UI heap.

## Build and run

Requirements: Windows 10 2004 or later, x64, .NET 10 SDK, Visual Studio Build Tools
with Desktop development with C++, Windows SDK 10.0.26100, Rust stable with the
`x86_64-pc-windows-msvc` target, Bun, and PowerShell. Visual Studio with WinUI support
can open `VniDrop.slnx`; the command line build also works.

From the repository root:

```powershell
pwsh windows/scripts/build.ps1 -Test -Run
```

The script builds Rust, installs the pinned C# binding generator when missing,
generates bindings and localized resources, runs the bridge tests, and builds the
app. `-Run` uses `build/windows/dev-profile` by default, keeping development
transfers separate from the installed app. Pass `-ProfileDirectory` to choose a
different profile. Subsequent UI-only builds can use `-SkipCore`.

```powershell
pwsh windows/scripts/build.ps1 -SkipCore -Run
pwsh windows/scripts/build.ps1 -Configuration Release -Test -Publish
```

Publishing writes an unpackaged, self-contained x64 app to `build/windows/publish`.
Keep the complete output directory together. It includes .NET, WinUI, resources,
and `vnidrop_native.dll`. The Rust DLL deliberately has a different name from the
managed `VniDrop.dll`, since Windows filenames are case-insensitive.

The SDK and Bun may alternatively be provided in the ignored repository-local
tool directories used by `build.ps1`. They are development prerequisites, not
checked-in binaries. Native bindings are also generated into ignored `build/`;
`windows/uniffi.toml` and the generator commit in `build-core.ps1` define the ABI.

## Verification

```powershell
pwsh windows/scripts/build.ps1 -SkipCore -Test
powershell.exe -NoProfile -File windows/scripts/smoke-ui.ps1 -Executable windows/VniDrop/bin/Debug/net10.0-windows10.0.26100.0/win-x64/VniDrop.exe
```

The bridge suite uses two real Rust endpoints in isolated local-only profiles.
It covers folder contents, Unicode paths, empty files, no-overwrite receiving,
cancellation while awaiting approval, saved-device consent, targeted receiving,
and identity preservation after shutdown. Presentation tests cover invitations,
draft state, argument parsing, plural forms, and legacy preferences.

The UI smoke test requires an interactive Windows desktop. It verifies native
startup, navigation resource names, modal transfer flows, settings persistence,
single-instance redirection, `.vnd` activation, invalid invitation feedback,
and shutdown. It leaves its isolated profile under `build/windows/smoke`.

For manual acceptance, create an approval-required share through the file picker,
export its `.vnd` invitation, receive it on a second device, approve the request,
and check the downloaded bytes. Also check light/dark mode, keyboard navigation,
high contrast, display scaling, and reduced window widths on supported Windows
versions. Windows may show its normal firewall prompt when the app first listens.

## State and localization

Direct executable launches use `%USERPROFILE%\.vnidrop`, matching the existing
Windows profile. Do not run the Compose and native hosts against the same profile
at the same time. Use `VniDrop.exe --profile <directory>` for another identity.
Repeated native launches with the same profile redirect to the existing window.

When `windows-preferences.json` does not exist, the app reads the existing Kotlin
DataStore preferences. It preserves the display name, receive folder, theme,
notification preference, relay policy, and diagnostics installation identifier.
Saving writes a separate JSON preferences file atomically. The Kotlin preferences
file is left intact. Invalid saved policies fail closed.

All visible strings come from `localization/strings.json`. The localization CLI
generates `Strings/<locale>/Resources.resw` for nine languages. Do not edit these
generated files. Windows-specific strings use `targets: ["windows"]`.

## Migration status

The native app implements send/receive history, file and folder drafts, invitation
review/import/export/copy, QR display, approval dialogs, cancellation, saved-device
management and targeted transfers, settings, relay configuration, cache management,
identity recovery, and local Windows notifications when supported by the host.
Closing the app confirms and stops active sharing; keeping a background process
in the notification area is not implemented yet.

This is a runnable native implementation alongside the existing shipping host.
The production Windows Store and EXE packaging workflows still build Compose.
Remaining cutover work includes native MSIX/installer packaging with the existing
Store identity and `.vnd` registration, notification activation in installed
packages, Windows share-sheet integration, camera QR scanning, and the full
Windows 10/11 accessibility and cross-device acceptance matrix. Double-click
file activation depends on an installed file association; command-line `.vnd`
activation and the in-app picker work in development builds.

Per-receiver byte progress, detailed activity timelines, and cancelling a draft
during preparation also need completion before replacing the shipping host.

Android/Linux continue using Compose; the Apple app and Rust core are unchanged.

Windows platform references: [WinUI 3](https://learn.microsoft.com/windows/apps/winui/winui3/),
[app activation](https://learn.microsoft.com/windows/apps/develop/launch/multi-instance-apps),
[notifications](https://learn.microsoft.com/windows/apps/develop/notifications/app-notifications/app-notifications-dotnet).
