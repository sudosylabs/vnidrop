# Application versioning

`version.properties` at the repository root is the single source of truth for
the VniDrop product version and the permanent Windows version epoch. Platform
projects and normal release workflows derive their versions from it rather than
accepting independent release counters. Previews retain that base version and
use the preview workflow run number to identify successive test builds.

Keep it as plain `KEY=VALUE` assignments: the same file is parsed by shell,
PowerShell, Gradle, and Rust. Xcode receives resolver-generated xcconfig files.

The product uses numeric semantic versions. While the app is in beta, feature
releases increment the minor component (`0.2.0`, `0.3.0`) and fixes increment
the patch component (`0.2.1`). Release channels belong in
`RELEASE_CHANNEL`; they are not appended to store version fields.

| Platform | Product version | Platform build/package version |
| --- | --- | --- |
| Android | `PRODUCT_VERSION` | Derived monotonic integer |
| Apple Store | `PRODUCT_VERSION` | Derived UTC `YYYYMMDD.HHMM.SS` |
| Direct macOS | `PRODUCT_VERSION` | Independently derived UTC `YYYYMMDD.HHMM.SS` |
| Linux | `PRODUCT_VERSION` | Native package revision |
| Rust handshake | `PRODUCT_VERSION` | Rust crate version remains independent |
| Microsoft Store | `PRODUCT_VERSION` in the app | Derived MSIX dot-quad |

MSIX requires a non-zero first component and reserves the fourth component for
the Store. Its version is:

```text
(product major + WINDOWS_VERSION_EPOCH).product minor.product patch.0
```

With epoch `1`, product `0.2.0` maps to MSIX `1.2.0.0`, while product `1.0.0`
maps to `2.0.0.0`. Do not change the epoch after publishing.

Android derives its version code as:

```text
product major * 1,000,000 + product minor * 1,000 + product patch
```

For example, `0.2.0` maps to Android code `2000`, `0.2.1` to `2001`, and
`1.0.0` to `1000000`. To keep that mapping unique and within store limits,
the product major may not exceed `2099`, and minor and patch may not exceed
`999`. A rejected store build must use a new patch version rather than
rebuilding a previously uploaded product version.

Apple build numbers are derived at build time by `apple-store-build` and
`apple-direct-build`; they are kept as separate resolver outputs so App Store
and Sparkle releases do not consume each other's cadence. Every changed Windows
Store package must increment the product version because the Store-reserved
fourth component cannot carry a rebuild number.

Apple projects read generated build settings rather than `version.properties`
directly:

```bash
packaging/version/generate-apple-xcconfig.sh all
```

The generated files under `apple/Generated/` are intentionally ignored.
`VNIDROP_BUILD_TIME_UTC=YYYYMMDDHHMMSS` provides a deterministic clock for
tests; distribution builds normally use the current UTC time.

Prepare the next release by changing only the product version:

```bash
make prepare-release RELEASE_VERSION=0.2.1
```

The command refuses non-increasing versions, updates `PRODUCT_VERSION`, and
prints the derived Android and Microsoft Store versions. Then verify:

```bash
make check-version
```

Normal release tags must exactly match `vPRODUCT_VERSION`. Manual workflow
dispatches also build the committed version and do not accept free-form version
inputs.

## Preview versions

The independent preview workflow publishes `preview-PRODUCT_VERSION-N`, where
`N` is its GitHub Actions run number. Assets include `PRODUCT_VERSION-preview.N`
in their filenames, and `preview-manifest.json` records the source commit.
Do not edit `version.properties` just to dispatch another preview.

Android uses version name `PRODUCT_VERSION-preview.N` and version code `N` for
the separate `com.vnidrop.app.preview` app, signed with its dedicated key. Keep
that key and workflow run numbering stable so preview upgrades remain possible.

Desktop package metadata keeps the numeric product version, and macOS also gets
its timestamp build number. A different preview of the same version may need a
package reinstall. Previews do not consume Store version numbers or update the
normal Sparkle/Homebrew feeds. See [Preview releases](../release/PREVIEW.md).
