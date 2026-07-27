# Releasing VniDrop for macOS

VniDrop ships on macOS through **two independent channels**:

| Channel | Target / config | Signing | Updates |
| --- | --- | --- | --- |
| **Mac App Store / TestFlight** | `VniDrop` / `Release` | Apple Distribution | App Store |
| **Direct download (.dmg)** | `VniDropDirect` / `Release-Direct` | Developer ID + notarization | Sparkle |

The direct build adds the **Sparkle** auto-updater, gated behind the
`DIRECT_DISTRIBUTION` compile flag so the App Store binary never links or ships a
self-updater (which Apple forbids). Because Swift Package Manager links products
per *target*, the isolation is a dedicated `VniDropDirect` target — not just a
build configuration.

This document covers the **direct-download** channel. The App Store channel goes
through Xcode Organizer / App Store Connect as before (see
`RELEASE-TESTFLIGHT.fr.md`).

## Versioning

Both channels use Apple's two-field convention, shown as `MARKETING_VERSION (build)`:

- **`CFBundleShortVersionString`** = `MARKETING_VERSION` — the human `X.Y.Z` version
  (from the git tag for a release). This is what users, the DMG filename, the
  GitHub Release, and the Homebrew cask all use.
- **`CFBundleVersion`** = a **UTC `YYMMDD.HHMM` timestamp**, stamped into the built
  Info.plist by the "Stamp build number" build phase (`apple/project.yml`). It fires
  for every build — Xcode Organizer archive and CLI alike — so each build is
  monotonic and self-describing. Sparkle compares this field to order updates, and
  App Store Connect requires each upload's build to exceed the previous one; a
  timestamp satisfies both automatically. `build-dmg.sh` pins one timestamp per
  archive (via `VNIDROP_BUILD`) so the app, DMG, and appcast agree.

No build number is maintained by hand.

---

## One-time setup

### 1. Developer ID Application certificate

Direct distribution requires a **Developer ID Application** certificate (distinct
from the *Apple Distribution* cert used for the App Store). In Xcode ▸ Settings ▸
Accounts ▸ Manage Certificates ▸ **+** ▸ *Developer ID Application*. Confirm it's
in the keychain:

```bash
security find-identity -v -p codesigning | grep "Developer ID Application"
```

### 2. Notarization credentials (App Store Connect API key)

Create an API key at App Store Connect ▸ Users and Access ▸ Integrations ▸ Keys
(role: *Developer*). Download the `AuthKey_XXXX.p8`, and note the **Key ID** and
**Issuer ID**. Store a local notarytool profile:

```bash
xcrun notarytool store-credentials vnidrop-notary \
  --key /path/to/AuthKey_XXXX.p8 \
  --key-id <KEY_ID> \
  --issuer <ISSUER_ID>
```

### 3. Sparkle EdDSA signing key

Generate the update-signing key once (private key stored in the login keychain):

```bash
# Sparkle's generate_keys, from the downloaded Sparkle release (bin/generate_keys)
./bin/generate_keys
```

It prints the **public** key. Put it in `apple/VniDrop/Resources/Info.plist` as
`SUPublicEDKey`, replacing `REPLACE_WITH_SPARKLE_ED_PUBLIC_KEY`. Export the
**private** key for CI:

```bash
./bin/generate_keys -x sparkle_ed_private_key   # writes the private key file
```

### 4. Homebrew tap repo

Create an empty GitHub repo **`sudosylabs/homebrew-vnidrop`** (a `Casks/` folder
is enough). The release workflow pushes `Casks/vnidrop.rb` there. Users install:

```bash
brew install --cask sudosylabs/vnidrop/vnidrop
```

### 5. CI secrets

Add these to the `sudosylabs/vnidrop` repo (Settings ▸ Secrets ▸ Actions):

| Secret | Value |
| --- | --- |
| `DEVELOPER_ID_CERT_P12` | base64 of the exported Developer ID `.p12` |
| `DEVELOPER_ID_CERT_PASSWORD` | password for that `.p12` |
| `NOTARY_API_KEY` | base64 of `AuthKey_XXXX.p8` |
| `NOTARY_KEY_ID` | App Store Connect key ID |
| `NOTARY_ISSUER` | App Store Connect issuer ID |
| `SPARKLE_ED_PRIVATE_KEY` | contents of the exported Sparkle private key file |
| `HOMEBREW_TAP_TOKEN` | PAT with write access to `homebrew-vnidrop` |

> `SUFeedURL` in Info.plist points at
> `https://github.com/sudosylabs/vnidrop/releases/latest/download/appcast.xml`.
> GitHub's `/releases/latest/download/` path always redirects to the newest
> non-prerelease release's asset, so no GitHub Pages or repo commits are needed.

---

## Per-release flow

1. Bump `MARKETING_VERSION` (and `CURRENT_PROJECT_VERSION`) in `apple/project.yml`
   if needed, commit to `master`.
2. Tag and push:

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

3. `.github/workflows/apple-release.yml` then:
   - builds the Rust core (release), regenerates the project,
   - archives + exports the `VniDropDirect` target (Developer ID, hardened runtime),
   - builds, signs, **notarizes and staples** `VniDrop-<version>.dmg`,
   - runs `generate_appcast` to produce `appcast.xml` (enclosure → the release DMG),
   - creates the GitHub Release with both assets, and
   - renders + pushes the Homebrew cask to the tap.

Existing installs pick up the new version automatically via Sparkle (feed →
latest release's `appcast.xml`); `brew upgrade` respects `auto_updates true`.

---

## Building a DMG locally

```bash
# Signed DMG (skips notarization unless NOTARY_PROFILE is set):
make build-apple-dmg VERSION=0.2.0

# Full signed + notarized DMG:
NOTARY_PROFILE=vnidrop-notary make build-apple-dmg VERSION=0.2.0
```

Output: `apple/dist/VniDrop-<version>.dmg`. See `apple/scripts/build-dmg.sh` for
the environment variables (`DEVELOPER_ID_APP`, `DEVELOPMENT_TEAM`,
`NOTARY_PROFILE`). Generate the appcast with
`apple/scripts/generate-appcast.sh <version>`.

To compile-check the direct target without signing:

```bash
make build-apple-macos-direct
```
