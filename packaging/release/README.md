# Manual releases

Use **Actions → Release → Run workflow**, leave the workflow branch on `master`,
enter the existing version tag, select platforms, and choose a distribution.
All platforms and **Direct downloads + stores** are selected by default. The
choices are **Direct downloads**, **Stores**, and **Direct downloads + stores**
(called `direct`, `stores`, and `both` below). Pushing a version tag does
not launch a release. [Preview releases](PREVIEW.md) have separate platform
checkboxes and publish direct downloads only.

The manual `.github/workflows/release.yml` verifies that
the tag matches `version.properties` and points to a commit on `master`, then
resolves the tag to a commit, and calls only the selected platform build workflows
with that exact commit. Windows builds WinUI, Apple
builds SwiftUI, Linux builds GTK/libadwaita, and Android uses Compose. Linux
packages build on Ubuntu 24.04 and Fedora 43; the release workflow passes the
diagnostics configuration to both package jobs.

The release workflow runs only when the repository variable
`RELEASE_PIPELINE_ENABLED` is exactly `true`. Leave it unset or set it to
`false` to disable all coordinated releases, including Play uploads, without
disabling release validation on pull requests.

| Platform | `direct` | `stores` | `both` |
| --- | --- | --- | --- |
| Windows | Unsigned EXE | Microsoft Store submission | Both |
| Linux | DEB and RPM | No destination | DEB and RPM |
| Android | Play-signed APK; no track changes | Closed-test draft | APK and closed-test draft |
| macOS | Signed, notarized DMG | Mac App Store upload | Both |
| iOS | No destination | App Store Connect / TestFlight upload | Store upload |

Unsupported destinations are skipped (for example iOS with `direct`). A selection
with no applicable destinations fails preflight. Windows with `direct` never
builds macOS or publishes a DMG. Windows with `both` publishes the unsigned EXE
and submits the MSIX upload to Microsoft Store.

Platform workflows upload workflow artifacts. Selected store publication paths
depend on their own platform builds, retaining their protected environments:

- Play stages the signed AAB as a draft on the configured closed-test track and
  downloads the universal APK signed by Play.
- Microsoft submits the unsigned `.msixupload` package for Store certification.
- Apple uploads only the selected iOS/macOS builds through the protected
  `apple-appstore` environment. It reuses the DMG job's core when available;
  store-only runs build their core from the selected tag without requiring a DMG
  or a previously published GitHub Release.

Once all selected direct packages are available, the GitHub job verifies and
assembles only those artifacts, generates checksums and provenance attestations,
uploads into a draft, then publishes one GitHub Release. It rejects failed or
cancelled required builds while allowing unselected jobs to be skipped. It does
not wait for Microsoft or Apple submissions. Website deployment follows GitHub
publication; Homebrew updates only when a new direct macOS package is included.
Store review and public
availability are separate from uploading a build.

Normal Android APKs retain the Play app-signing certificate. Even `direct` needs
the Play credentials and protected `play-closed-testing` environment: it uploads
the AAB to Play to generate the signed APK, without assigning it to a track.
It never substitutes the upload-key-signed APK. `stores` and `both` retain the
existing draft-only closed-testing policy.

Each new source release uses a new product version. For a Windows/Linux-only
`0.3.5` after an all-platform `0.3.4`, the new GitHub Release contains the Windows
and Linux packages. Its manifest's `downloads` index retains the original `0.3.4`
macOS and Android URLs and checksums. The website shows the version and checksum
link beside each download. Older binaries are not copied or relabelled.
The previous `appcast.xml` is copied byte-for-byte after checksum verification
when macOS is skipped, preserving the feed used by installed Macs. A normal
release must advance the last published product version. Preview manifests
cannot become the normal download index.

A manual Apple run with `release_tag` checks out that tag, verifies its version,
and downloads its published core. Both app builds use the resolved commit SHA.
A missing or invalid core download fails the run; leave `release_tag` blank to
explicitly build a core from the selected workflow revision instead.

Android release packaging lints the Release variant and verifies native libraries
and signatures in the APK and AAB. Shared KMP CI runs the app's full `check` task,
including debug unit tests and debug APK verification, alongside the Gradle
diagnostics tests. `make check-release` exercises packaging and publication
scripts without repeating those Gradle builds.

Public GitHub Release assets are the DEB, RPM, notarized DMG, Sparkle appcast,
Play-signed universal APK, unsigned Windows direct installer,
`VnidropCore-<version>.zip`, checksum file, and release manifest.

The unsigned Microsoft `.msixupload` and upload-signed Android AAB remain
private workflow artifacts. The protected `microsoft-store` GitHub Environment
supplies the Partner Center credentials and Store product ID used to submit the
Windows package. Microsoft publishes the update after certification; the job
does not change Store listings, pricing, or availability. The Play release
remains a draft on a closed-testing track; this pipeline cannot publish it to
production.

The public Windows `.exe` is intentionally unsigned. Windows SmartScreen is
expected to warn that the publisher is unknown or that the app might be
dangerous. Release users should verify the installer against `SHA256SUMS`; the
Microsoft Store remains the signed installation path.

To release, prepare and merge the new product version. Android, Microsoft Store,
and Apple build/package versions are derived automatically:

```bash
make prepare-release RELEASE_VERSION=0.2.1
make check-version
```

Then create and push the matching tag (this only records the version):

```bash
git tag -s v0.2.1 -m "VniDrop 0.2.1"
git push origin v0.2.1
```

Launch manually from the Actions form, or use the CLI. For Windows and Linux
direct downloads only:

```bash
gh workflow run release.yml --ref master \
  -f release_tag=v0.2.1 -f distribution='Direct downloads' \
  -F windows=true -F linux=true -F android=false -F macos=false -F ios=false
```

For Windows EXE plus Microsoft Store, select only Windows and **Direct downloads + stores**. For a full
release, leave every platform selected and choose **Direct downloads + stores**. Store-only runs do not
create a GitHub download release or update the website/Homebrew.

The tagged commit must be an ancestor of `origin/master`; later merges do not
invalidate the tag or its retries. A failure before the Create GitHub Release
step leaves no GitHub Release, but Play, Microsoft Store, or App Store Connect
may already have received a submission. Check those services before retrying.
Play draft reuse requires the same version, configured track, draft status, and
app-signing certificate.

If the Create GitHub Release step fails, check for an incomplete draft. Remove
that unpublished draft before retrying publication; retain the source tag.
Never replace assets or move the tag of an already published release.

A failure after publication leaves the GitHub Release and its assets in place.
Rerun the failed jobs after checking their submission or deployment state.
Rerunning the entire workflow fails preflight because the release already exists.
Keep the existing release and tag when recovering Apple uploads, website
deployment, or the Homebrew update.
