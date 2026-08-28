# Windows packaging

This directory turns the Compose Desktop Windows app image into the unsigned
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

The workflow explicitly selects Gobley's release Rust variant and rejects a
package containing the debug native JAR. It verifies that the direct installer
is unsigned, and also verifies the bundled JVM, vnidrop.dll, app version,
manifest identity, architecture, and launcher after MakeAppx unpacks the
finished package.

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

> VniDrop is a classic JVM desktop application that loads its bundled native
> Rust and JVM libraries and needs normal user-level filesystem and network
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
.\gradlew.bat :shared:jvmTest :desktopApp:createReleaseDistributable :desktopApp:packageReleaseExe -Pvnidrop.desktop.rustVariant=release -Pvnidrop.diagnostics.included=false --no-daemon --no-configuration-cache --stacktrace

$directInstaller = Get-ChildItem .\desktopApp\build\compose\binaries\main-release\exe\*.exe
if (@($directInstaller).Count -ne 1) { throw "Expected exactly one direct installer" }
.\packaging\windows\build-msix.ps1 -AppImage .\desktopApp\build\compose\binaries\main-release\app\VniDrop -DirectInstaller $directInstaller.FullName -OutputDirectory .\build\release\windows
~~~

The packaging script requires Windows SDK 10.0.26100.0. It uses MakePri to
index the scale-qualified visual assets, then MakeAppx with SHA-256 block maps
and manifest validation enabled.

Microsoft references:

- [MSIX Store package requirements](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)
- [Manual desktop MSIX packaging](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
- [MakeAppx](https://learn.microsoft.com/en-us/windows/msix/package/create-app-package-with-makeappx-tool)
- [Uploading MSIX packages](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/upload-app-packages)
- [GitHub Actions Store updates](https://learn.microsoft.com/en-us/windows/apps/publish/msstore-dev-cli/github-actions)
