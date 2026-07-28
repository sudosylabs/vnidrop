# Application versioning

`version.properties` at the repository root is the single source of truth for
the VniDrop application version. Platform projects and release workflows read
that file rather than accepting independent version overrides.

Keep it as plain `KEY=VALUE` assignments: the same file is parsed by shell,
PowerShell, Gradle, Rust, and Xcode.

The product uses numeric semantic versions. While the app is in beta, feature
releases increment the minor component (`0.2.0`, `0.3.0`) and fixes increment
the patch component (`0.2.1`). Release channels belong in
`RELEASE_CHANNEL`; they are not appended to store version fields.

| Platform | Product version | Platform build/package version |
| --- | --- | --- |
| Android | `PRODUCT_VERSION` | `ANDROID_VERSION_CODE` |
| Apple | `PRODUCT_VERSION` | `APPLE_BUILD_NUMBER` |
| Linux and direct macOS | `PRODUCT_VERSION` | Native package revision |
| Rust handshake | `PRODUCT_VERSION` | Rust crate version remains independent |
| Microsoft Store | `PRODUCT_VERSION` in the app | Derived MSIX dot-quad |

MSIX requires a non-zero first component and reserves the fourth component for
the Store. Its version is:

```text
(product major + WINDOWS_VERSION_EPOCH).product minor.product patch.0
```

With epoch `1`, product `0.2.0` maps to MSIX `1.2.0.0`, while product `1.0.0`
maps to `2.0.0.0`. Do not change the epoch after publishing.

Every Android or Apple upload must increment its platform build number. Every
changed Windows Store package must increment the product version because the
Store-reserved fourth component cannot carry a rebuild number.

Before releasing:

```bash
make check-version
```

Release tags must exactly match `vPRODUCT_VERSION`. Manual workflow dispatches
also build the committed version and do not accept free-form version inputs.
