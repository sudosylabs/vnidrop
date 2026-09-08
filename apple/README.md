# VniDrop — native Apple app (iOS / iPadOS / macOS)

A native SwiftUI app for Apple platforms, sharing the existing Rust transfer core
(`crates/vnidrop`) through UniFFI-generated Swift bindings. SwiftUI owns the
screens and Apple platform integration. Android and the released Linux app use
Compose; Windows uses WinUI. A native GTK/libadwaita Linux app is also under
development. See the [platform overview](../doc/architecture/platforms.md).

## Layout

```
apple/
  scripts/build-core.sh     # builds the Rust core + generates Swift bindings + xcframework
  VnidropCore/              # local SwiftPM package: xcframework + generated Vnidrop.swift
  VniDrop/                  # SwiftUI app sources
    App/                    # entry point, object graph, root view, environment
    Core/                   # repository, models, preferences, notifications, progress
    Features/Send|Receive|Approvals|Settings/
    UI/Theme|Components|Navigation|Feedback|Shell/
    Platform/               # pickers, QR, NFC, share/export, per-OS file services
    Resources/              # Localizable.xcstrings, Info.plist, entitlements, assets
  Tests/                    # XCTest bundle (VniDropTests target)
  project.yml               # XcodeGen spec for the iOS/macOS app and test targets
```

## Build & run

Prerequisites: Xcode, Rust with the Apple targets
(`aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios`,
`aarch64-apple-darwin`), Bun for localization, XcodeGen, and SwiftLint
(`brew install xcodegen swiftlint`).

```bash
# From the repository root:
make apple-core          # Rust core, Swift bindings, and XCFramework
make apple-project       # generate apple/VniDrop.xcodeproj
make open-apple-project  # generate and open the project in Xcode
make build-apple-macos   # unsigned macOS build (App Store target)
make open-apple APPLE_CODE_SIGNING=YES  # launch after configuring Local.xcconfig
make build-apple-ios     # unsigned iOS simulator app
make check-apple         # iOS simulator tests
```

`make apple-project` also generates ignored Store and Direct version xcconfig
files. Their `CURRENT_PROJECT_VERSION` values come from the central version
resolver as UTC `YYYYMMDD.HHMM.SS` build identifiers. Regenerate the project
before creating another App Store archive so it receives a fresh build number;
direct DMG builds refresh their own value automatically.

### macOS shipping channels

The macOS app ships through two targets that build identical sources:

- **`VniDrop`** (`Release`) — Mac App Store / TestFlight. Sandboxed, no
  self-updater.
- **`VniDropDirect`** (`Release-Direct`) — direct-download `.dmg` on GitHub
  Releases + Homebrew cask. Adds the **Sparkle** auto-updater behind the
  `DIRECT_DISTRIBUTION` compile flag, so the App Store binary never links Sparkle.

```bash
make build-apple-macos-direct   # unsigned compile-check of the direct target
make build-apple-dmg                # signed (+ notarized) .dmg
```

The [DMG workflow](../.github/workflows/apple-release.yml) configures Developer ID
signing, notarization, and Sparkle appcast signing. The
[App Store workflow](../.github/workflows/apple-appstore.yml) archives and uploads
iOS and macOS builds. See [Coordinated releases](../packaging/release/README.md)
for publication order, the Homebrew update, and recovery after a failed job.

Use `APPLE_PROFILE=release` to request a release Rust core, or set
`APPLE_DESTINATION` to override the automatically selected iOS simulator.
Code signing is disabled for compile-only build targets; local and CI builds do
not require an Apple Development team or provisioning profile. To launch the
macOS app, configure the ignored `apple/Local.xcconfig` and run
`make open-apple APPLE_CODE_SIGNING=YES`; `open-apple` rejects unsigned builds
because protected Keychain custody is unavailable without the app entitlements.
`make check-apple` signs ad-hoc
(`CODE_SIGN_IDENTITY=-`, override with `APPLE_TEST_CODE_SIGN_IDENTITY`) because
the core keeps its endpoint identity in the protected keychain, which an
unsigned simulator app cannot reach. Ad-hoc needs no certificate or team. For
signed builds from Xcode, create the ignored `apple/Local.xcconfig` and override
the signing settings there, including the development team.

## Typecheck & tests

The Xcode project is the only build definition: it owns the UI, its package
dependencies, and the `VniDropTests` bundle (module `VniDrop`, which is what the
tests import). Everything runs through `xcodebuild`:

```bash
make check-apple         # iOS simulator unit tests
make build-apple-macos   # unsigned macOS build (typecheck)
make open-apple APPLE_CODE_SIGNING=YES  # signed local macOS run
```

There is deliberately no SwiftPM manifest for the app. A second build definition
would duplicate the target's package dependencies, and the previous one had
already drifted out of sync with `project.yml` badly enough that neither
`swift build` nor `swift test` worked.

## Generated / ignored artifacts

`build-core.sh` produces build outputs that are gitignored (see `apple/.gitignore`):
`VnidropCore/vnidrop.xcframework/`, `VnidropCore/Sources/VnidropCore/Vnidrop.swift`,
and `.build-core/`. A clean checkout must run `build-core.sh` before generating or
opening the Xcode project. `VniDrop.xcodeproj` itself is generated by XcodeGen from
`project.yml` and does not need to be committed.

## Build profile note

The default is `debug`. `build-core.sh` sets `CARGO_PROFILE_DEV_STRIP=none` and
`CARGO_PROFILE_RELEASE_LTO=false` to avoid corrupt host proc-macro dylibs seen
when cross-compiling on macOS ("mis-aligned LINKEDIT string pool"). These
overrides apply to the build command; no local Cargo profile edit is needed.
Release builds omit simulator slices by default. Set `VNIDROP_APPLE_SIMULATOR=1`
when a release core must also support simulator development.

## System frameworks

The Rust core (iroh network stack) links `SystemConfiguration`, `Security`, and
`libresolv`. These are declared in `project.yml` for the app target.

## Parity & scope

Apple and KMP share transfer and consent semantics through the Rust core and the
[Saved Devices UI contract](../shared/docs/saved-devices-ui-contract.md).
Apple presentation uses SwiftUI controls, iOS tabs, a macOS sidebar, and SF Symbols
for empty states.

## Bug reporting

The Settings report form submits to the existing diagnostics API on iOS and macOS.
Reports include the entered description, app/device information, and an anonymous
installation ID. Recent Rust core logs are optional and off by default; tickets,
endpoint IDs, paths, and addresses are redacted before upload. There is no telemetry
or automatic crash reporting. Failed submissions keep the draft and reuse the same
report ID when retried during the app session.

`apple/scripts/generate-appconfig.sh` embeds `VNIDROP_DIAGNOSTICS_ENDPOINT` (the
HTTPS service base URL) and `VNIDROP_DIAGNOSTICS_INGEST_KEY` in the ignored
`VniDrop/Generated/AppConfig.swift`. The key is the distributable ingestion key,
not an administrative credential. Do not commit the generated configuration.
Unconfigured development builds show a configuration error when a report is sent.

Both `apple-release.yml` (DMG) and `apple-appstore.yml` (iOS/macOS App Store and
TestFlight) read the endpoint repository variable and ingestion-key secret, and
set `VNIDROP_REQUIRE_DIAGNOSTICS=1` so missing configuration fails the build.
Generate the project again after changing either value. Apple CI uses fixture
configuration and intercepted HTTP requests; it never sends production reports.

Validation: `apple/scripts/tests/test-generate-appconfig.sh`, `make check-apple`
(iOS simulator tests), and `make build-apple-macos-direct` (direct macOS target).
