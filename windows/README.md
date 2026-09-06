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

## Invitation file association

Register the unpackaged native app as a `.vnd` handler for the current Windows user:

```powershell
pwsh windows/scripts/build.ps1 -SkipCore -RegisterFileAssociation -Run
```

This registers the development executable with `build/windows/dev-profile` (or
`-ProfileDirectory`), so Explorer opens invitations in the same profile and redirects
to its existing window. Registration is explicit; ordinary builds and launches do
not change it. An unclaimed `.vnd` extension gets an initial native mapping.
Windows may still ask you to choose **VniDrop** and **Always** on the first open.
Existing defaults are preserved; change them through **Open with** or
**Settings > Apps > Default apps**, searching for `.vnd`.

To register a published app at its final location, or remove its registration:

```powershell
pwsh windows/scripts/register-file-association.ps1 -Executable C:/Apps/VniDrop/VniDrop.exe
pwsh windows/scripts/register-file-association.ps1 -Executable C:/Apps/VniDrop/VniDrop.exe -Unregister
```

Without `-ProfileDirectory`, the published handler uses `%USERPROFILE%\.vnidrop`.
Keep the full app directory at the registered location; rerun registration after
moving it. Unregister before removing the app. An old executable's unregister
command cannot remove a replacement build's registration. No administrator rights
are needed. This is for development builds. The release EXE registers its handler
during installation; MSIX uses manifest associations. Installed users do not run
this script. See [Windows packaging](../packaging/windows/README.md).

## Verification

```powershell
pwsh windows/scripts/build.ps1 -SkipCore -Test
powershell.exe -NoProfile -File windows/scripts/test-file-association.ps1
powershell.exe -NoProfile -File windows/scripts/smoke-ui.ps1 -Executable windows/VniDrop/bin/Debug/net10.0-windows10.0.26100.0/win-x64/VniDrop.exe
```

The bridge suite uses two real Rust endpoints in isolated local-only profiles.
It covers folder contents, Unicode paths, empty files, no-overwrite receiving,
cancellation during preparation and while awaiting approval, saved-device consent, targeted receiving,
and identity preservation after shutdown. Presentation tests cover invitations,
draft state, argument parsing, plural forms, and legacy preferences.

For Rust core changes, run the workspace checks from the repository root. Limiting
test concurrency on Windows keeps the many simultaneous test endpoints from
competing for local networking resources:

```powershell
$env:RUST_TEST_THREADS = '4'
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features integration-test-store -- -D warnings
cargo test --workspace --all-targets --features integration-test-store
$env:RUSTDOCFLAGS = '-D warnings'
cargo doc --workspace --no-deps
```

File-association tests use an isolated registry subtree. They check Windows command
line quoting (spaces, Unicode, and profile roots), registration updates, preserving
other handlers and user defaults, and removal without affecting a newer build.
For Explorer acceptance, open a `.vnd` file while the app is closed, close its
invitation review, then open it again while the app is running. Both opens should
show the receive review in one window. An invalid invitation must show an error
without starting a transfer; a valid invitation must still require confirmation.
Generate valid and invalid fixtures without using personal transfers (choose a new
directory for each run):

```powershell
dotnet run --project windows/scripts/fixtures/FileActivation/FileActivation.csproj -- build/windows/activation-fixture
```

The fixture sender stops after export; verify the valid invitation's file name,
size, and sender in the review, then cancel without starting a download.

The UI smoke test requires an interactive Windows desktop. It verifies native
startup, navigation resource names, modal transfer flows, settings persistence,
single-instance redirection, `.vnd` activation, invalid invitation feedback,
and shutdown. It leaves its isolated profile under `build/windows/smoke`.

Transfer-detail layout and scrolling checks use an isolated 22-file transfer with
12 completed receivers. Choose a new fixture directory for each run:

```powershell
dotnet run --project windows/scripts/fixtures/TransferDetails/TransferDetails.csproj -- build/windows/details-fixture
powershell.exe -NoProfile -File windows/scripts/smoke-transfer-details.ps1 -Executable windows/VniDrop/bin/Debug/net10.0-windows10.0.26100.0/win-x64/VniDrop.exe -ProfileDirectory build/windows/details-fixture/sender
```

These checks cover separate metadata, action alignment at 1200/800/500 effective
pixels, and internal history scrolling with fixed dialog controls after resizing.

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

File selection uses Windows thumbnails and associated file icons. Invitation
shares keep a bounded preview under `ui/previews`, compatible with the KMP cache,
so their artwork survives a restart or a moved source file. Received transfers
load artwork from their published files. Older native shares without cached
artwork use a file-type icon; their original source paths were not retained.

All visible strings come from `localization/strings.json`. The localization CLI
generates `Strings/<locale>/Resources.resw` for nine languages. Do not edit these
generated files. Windows-specific strings use `targets: ["windows"]`.

## Migration status

The native app implements send/receive history, file and folder drafts with
preparation cancellation, invitation
review/import/export/copy, QR display, approval dialogs, cancellation, saved-device
management and targeted transfers, settings, relay configuration, cache management,
identity recovery, and local Windows notifications when supported by the host.
Per-receiver progress and activity timelines are implemented. Closing the app
confirms and stops active sharing. Transfers continue while the app remains open;
notification-area background operation and desktop camera QR scanning are outside
the Windows release scope.

The Windows Store and EXE release workflow now builds the native app, preserving
the existing Store and MSI upgrade identities and registering `.vnd` at installation.
The separate Native Windows workflow still provides a development publish folder.
The MSIX declares the app-specific notification COM activator. Package acceptance
checks cover running-instance and cold-start notification activation through the
Windows notification COM contract, alongside packaged startup. Real notification
delivery/clicks, Windows share-sheet integration, and the full Windows 10/11
accessibility and cross-device acceptance matrix still require interactive testing.
The hosted-runner cold-notification check is not yet passing; see the
[package acceptance blocker](../packaging/windows/README.md) before releasing.
Unpackaged native builds support double-click activation after the explicit registration above;
command-line `.vnd` activation and the in-app picker also work without registration.

The package workflow tests the checksum-pinned, published Compose 0.3.3 installer
as a same-version upgrade, with an existing profile fixture containing legacy
preferences, protected identity, transfer history and received file bytes. It
verifies preservation after the new app runs and again after uninstall. The
fixture creates a real core profile and Compose-format preferences; it does not
replace testing an actual user's migrated profile on supported Windows versions.

Android/Linux continue using Compose, and Apple continues using SwiftUI. All
hosts share the Rust transfer core.

Windows platform references: [WinUI 3](https://learn.microsoft.com/windows/apps/winui/winui3/),
[app activation](https://learn.microsoft.com/windows/apps/develop/launch/multi-instance-apps),
[notifications](https://learn.microsoft.com/windows/apps/develop/notifications/app-notifications/app-notifications-dotnet).
