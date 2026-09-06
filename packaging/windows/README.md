# Windows packaging

This directory turns the native WinUI 3 Release publish output into the unsigned
MSIX artifacts accepted by Partner Center. Microsoft signs the package after
certification, so this build does not use a PFX, certificate, HSM, or signing
secret. The same workflow also produces an intentionally unsigned `.exe` for
people who prefer to install VniDrop directly from the GitHub Release.

## Product identity

These values came from the Partner Center Product identity page and are
case-sensitive:

| Field | Value |
| --- | --- |
| Package identity name | SudosyLabs.Vnidrop |
| Publisher | CN=6456DC8E-2C31-44BD-AACC-2E6813C833CB |
| Publisher display name | Sudosy Labs |
| Reserved Store name | Vnidrop |
| Application ID | VniDrop |
| Store ID | 9NJ5Q0FG7TGL |

The package identity name, publisher, and Application ID must remain stable
after the first release. The manifest display name uses the exact reserved Store
name; the product's in-app branding and launcher remain `VniDrop`.

The initial package targets Windows Desktop x64, Windows 10 version 2004
(build 19041) or later. The product version comes from `version.properties`.
Because MSIX requires a non-zero major and reserves the fourth component, the
package version adds `WINDOWS_VERSION_EPOCH` to the product major. With epoch
`1`, product version `0.2.0` becomes package version `1.2.0.0`.

## GitHub Actions

The Windows package workflow runs automatically for relevant pull
requests and by manual dispatch. The coordinated release workflow also calls
it for a canonical `vMAJOR.MINOR.PATCH` tag. Pull requests build and validate
without retaining an artifact. Manual and coordinated-release runs retain:

- VniDrop_VERSION_x64.msix
- VniDrop_VERSION_x64.msixupload
- VniDrop_VERSION_x64.exe
- build metadata
- SHA-256 checksums

The preferred Partner Center upload is the msixupload file. It is an upload
envelope containing the x64 MSIX. The MSIX is intentionally unsigned and is not
a public sideloading artifact. Do not attach it to a public GitHub Release
unless an independent production-signing path is added.

The `.exe` is the public direct installer. It is intentionally unsigned, so
Windows SmartScreen is expected to show an unknown-publisher or potentially
dangerous-app warning. Users should download it only from the official GitHub
Release and compare it with the published `SHA256SUMS` before running it.

The workflow builds and tests the native Release app, including the Release Rust
library, .NET runtime, Windows App SDK, XAML and localized resources. It verifies
the unsigned installer, native version and runtime assets, Store identity, and
every published file's hash after extracting both the MSI and MSIX. The EXE's
embedded MSI must match the validated MSI exactly. PDBs are excluded.

The direct EXE is a WiX 4.0.6 Burn bundle containing a per-user MSI. The build
script installs the pinned WiX tool and bootstrapper extension under
`build/windows/tools/wix`; end users need neither WiX nor .NET installed.
The MSI retains the Compose upgrade code
`E08E256E-2F07-479E-8AA9-4898D424F6C5` and jpackage's `.vnd` ProgId. Major upgrades
remove the old runtime inside a rollback transaction, including same-version
Compose-to-WinUI replacement across installer languages. The new bundle has its
own stable upgrade code.
Installation adds Start menu and desktop shortcuts and registers VniDrop in
Open With and Default apps. It does not overwrite Windows UserChoice or another
application's default. Windows may ask the user to choose a handler once.
Uninstall removes installer-owned files and registrations, preserving profiles.

The MSIX retains the existing Store identity and manifest `.vnd` association.
It imports runtime registrations from the Windows App SDK manifest or fragments
selected by the native build. MakePri merges the already-compiled `VniDrop.pri`
with Store artwork once; scanning the framework payload again creates duplicate
resource entries. The original app PRI and XAML files remain intact.

## First Store release

Microsoft's current GitHub Actions publishing flow is for updates to an
already-live free product. For the first release:

1. Push a canonical release tag to run the coordinated workflow, or run the
   Windows package workflow manually.
2. Download the retained artifact.
3. Test those exact builds on an interactive Windows VM. Local MSIX installation
   needs an ephemeral development signature trusted only by that VM; this is
   not a production signing key. The direct `.exe` should display the expected
   SmartScreen warning and install for the current user.
4. Upload the msixupload file to the current Partner Center draft.
5. Confirm that Partner Center parses the expected identity, version, x64
   architecture, Windows.Desktop target, declared UI languages (`en-US`,
   `de-DE`, `es-ES`, `fr-FR`, `it-IT`, `nl-NL`, `pl-PL`, `pt-PT`, and
   `ru-RU`), and runFullTrust capability.
6. Complete listing, screenshots, certification notes, and submit.

Use this restricted-capability justification in Submission options:

> VniDrop is a native WinUI desktop application that loads its bundled native
> Rust library and needs normal user-level filesystem and network
> access to transfer user-selected files directly between devices.

After the first release is certified and live, the coordinated release
workflow submits the generated `.msixupload` from a separate protected job.
Keep its Partner Center credentials in the `microsoft-store` GitHub
Environment, not in the build job:

- AZURE_AD_TENANT_ID
- AZURE_AD_APPLICATION_CLIENT_ID
- AZURE_AD_APPLICATION_SECRET
- SELLER_ID

Set `MICROSOFT_STORE_PRODUCT_ID` to `9NJ5Q0FG7TGL` as a non-secret variable in
the same environment. The publishing job validates the product ID,
authenticates with the pinned Microsoft Store Developer CLI, verifies access to
the product, and submits only the package for certification. Existing listings,
pricing, and availability are preserved.

## Manual build on Windows

From the repository root:

~~~powershell
.\windows\scripts\build.ps1 -Configuration Release -Test -Publish
.\packaging\windows\build-installer.ps1 -AppImage .\build\windows\publish -OutputDirectory .\build\windows\installer
$version = (.\packaging\version\resolve-version.ps1 -Field Json | ConvertFrom-Json).productVersion
.\packaging\windows\build-msix.ps1 -AppImage .\build\windows\publish -DirectInstaller ".\build\windows\installer\VniDrop_${version}_x64.exe" -OutputDirectory .\build\release\windows
.\packaging\windows\test-installer.ps1 -AppImage .\build\windows\publish -InstallerDirectory .\build\windows\installer
~~~

The packaging script requires Windows SDK 10.0.26100.0 and the native build
prerequisites in [`windows/README.md`](../../windows/README.md). It uses MakeAppx
with SHA-256 block maps and manifest validation enabled. MSI and build sources
remain under `build/windows/installer`; only the existing release artifact set
is copied into `build/release/windows`.

On a clean interactive test account, run the installation acceptance checks:

~~~powershell
.\packaging\windows\test-installer.ps1 -AppImage .\build\windows\publish -InstallerDirectory .\build\windows\installer -Install
powershell.exe -NoProfile -File .\packaging\windows\test-msix.ps1 -Package ".\build\release\windows\VniDrop_${version}_x64.msix"
~~~

The installer test refuses to replace an existing VniDrop installation. It
installs a small legacy MSI fixture, upgrades through the EXE, launches the
installed native app in an isolated profile, and checks uninstall and default
preservation. The MSIX test uses Developer Mode to register its extracted
payload temporarily, then verifies activation, localized navigation and removal.
CI enables Developer Mode only for that test and restores its previous setting.
Neither test signs or changes the release MSIX. Testing the real previous public
installer and a Store-delivered upgrade on Windows 10/11 remains a release
acceptance step; the legacy fixture does not replace those checks.

For diagnosing a failing bootstrapper, add `-MsiOnly` to the installer acceptance
command. This tests the embedded MSI directly and explicitly leaves EXE
installation unverified. The release workflow always tests the full EXE.

Microsoft references:

- [MSIX Store package requirements](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)
- [Manual desktop MSIX packaging](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
- [MakeAppx](https://learn.microsoft.com/en-us/windows/msix/package/create-app-package-with-makeappx-tool)
- [Uploading MSIX packages](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/upload-app-packages)
- [GitHub Actions Store updates](https://learn.microsoft.com/en-us/windows/apps/publish/msstore-dev-cli/github-actions)
