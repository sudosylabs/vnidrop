# Preview releases

Use **Actions → Preview release → Run workflow → master** after merging the
workflow. Each dispatch builds the selected `master` commit in Release
configuration and publishes a GitHub prerelease such as `preview-0.3.3-17`.
The run number distinguishes previews without changing `version.properties`.

The workflow is independent of `RELEASE_PIPELINE_ENABLED`, store approval
environments, and normal release concurrency. Its reusable builders also use
separate concurrency groups for each calling workflow. GitHub account runner
and minute limits still apply to both pipelines.

## Downloads

| Platform | Artifact | Installation |
| --- | --- | --- |
| macOS | Apple Silicon `.dmg` | Developer ID signed, notarized, and stapled; replaces the existing direct app |
| Windows | x64 `.exe` | Unsigned current-user installer; SmartScreen may warn |
| Linux | x64 `.deb` and `.rpm` | Existing GTK app identity; Ubuntu 24.04+ and Fedora 43 baselines |
| Android | arm64-v8a + x86_64 `.apk` | Signed with the preview key; separate `com.vnidrop.app.preview` installation |

Every build includes the normal bug-report configuration. No App Store,
Microsoft Store, or Play Store submission runs. macOS notarization uses Apple's
automated notary service, which is separate from App Review.

Windows skips MSIX generation and activation checks. macOS builds only its own
Rust core slice and skips the prebuilt core ZIP and Sparkle appcast. Android
lints and assembles the Release APK without building an AAB or Android debug
variant. Normal CI retains unit tests and debug-package checks.

The publisher checks each package's checksum, macOS build metadata, and Android
preview identity before creating downloads. It publishes only the five packages,
`preview-manifest.json`, and `SHA256SUMS`. The manifest records the exact commit,
preview number, Android certificate fingerprint, and macOS build number.

Previews use `--prerelease --latest=false`, and their tags begin with `preview-`
rather than `v`. They do not start the normal tag workflow or update the website,
Homebrew cask, or stable Sparkle feed. macOS preview installations continue to
check the stable feed and can move to a later stable build; preview updates are
downloaded manually from GitHub Releases.

## Version and installation behaviour

Android's version name includes `-preview.N`, and its version code is the
workflow run number. Keep the workflow's run numbering and signing key stable
so newer previews can update existing preview installations. Android requires
signed APKs even for distribution outside Play. The separate package ID avoids
certificate conflicts and keeps preview data separate from the store app.

Desktop binaries and package metadata keep the numeric product version from
`version.properties`; the preview identifier appears in download filenames and
the manifest. macOS also embeds its existing timestamp build number. Desktop
previews share the normal app's data and installation identity. Close VniDrop
before installing and retain any needed data backup before testing changes that
migrate local data. Moving between previews with the same product version may
require reinstalling the desktop package. A lower-version normal release may
require uninstalling the preview first; publishing a later normal version uses
the existing release procedure.

## One-time Android signing setup

Create one dedicated key and retain a secure backup outside the repository.
Do not generate a new key on every workflow run. For example, with a JDK and
GitHub CLI in Bash:

```bash
keytool -genkeypair -keystore "$HOME/vnidrop-preview.p12" -storetype PKCS12 \
  -alias vnidrop-preview -keyalg RSA -keysize 3072 -validity 10000

base64 < "$HOME/vnidrop-preview.p12" | gh secret set ANDROID_PREVIEW_KEYSTORE_BASE64 --repo sudosylabs/vnidrop
gh secret set ANDROID_PREVIEW_KEYSTORE_PASSWORD --repo sudosylabs/vnidrop
gh secret set ANDROID_PREVIEW_KEY_PASSWORD --repo sudosylabs/vnidrop
gh secret set ANDROID_PREVIEW_KEY_ALIAS --body vnidrop-preview --repo sudosylabs/vnidrop
keytool -list -v -keystore "$HOME/vnidrop-preview.p12" -alias vnidrop-preview
```

Enter the same password for the two password secrets with this PKCS12 setup.
Copy the certificate's SHA-256 fingerprint into the repository **variable**
`ANDROID_PREVIEW_CERT_SHA256` (colon-separated or plain hexadecimal).

The workflow reuses the existing Developer ID certificate, provisioning profile,
notary credentials, and diagnostics settings documented in [README.md](README.md).
It needs no Sparkle signing key or store submission credentials. Missing preview
configuration fails the inexpensive preflight before platform builds start.

## Reruns and validation

Re-run failed jobs when a build fails; successful artifacts from the same run
remain available until their retention expires. A new dispatch gets a new
preview number and tag. Published previews are immutable: never replace their
assets or move their tags.

Publication first uploads into a draft so an upload failure cannot expose an
incomplete preview. If it fails after creating that draft, inspect the draft,
remove the incomplete draft release and its preview tag, then re-run the failed
publication job. Do not delete a published preview to reuse its number. If
artifacts have expired, start a new workflow run.

Run these checks before merging:

```bash
make check-release
python3 packaging/android/tests/test_preview_config.py -v
```

The second command uses the installed Android SDK to verify real Release
manifests for both preview and store configurations. Signed macOS packaging and
the complete cross-platform build require the corresponding hosted runners.

References: [Android app signing](https://developer.android.com/studio/publish/app-signing),
[Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).
