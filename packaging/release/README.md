# Coordinated releases

Only `.github/workflows/release.yml` responds to version tags. It verifies that
the tag matches `version.properties` and points at the current `master`, then
calls the platform build workflows in parallel. Windows builds WinUI, Apple
builds SwiftUI, Linux builds GTK/libadwaita, and Android uses Compose. Linux
packages build on Ubuntu 24.04 and Fedora 43; the release workflow passes the
diagnostics configuration to both package jobs.

The tag workflow runs only when the repository variable
`RELEASE_PIPELINE_ENABLED` is exactly `true`. Leave it unset or set it to
`false` to disable all coordinated releases, including Play uploads, without
disabling release validation on pull requests.

Platform workflows upload private workflow artifacts. After every platform build
passes, the release pipeline:

1. stages the signed AAB as a draft on the configured Play closed-test track;
2. downloads the universal APK signed by Play;
3. submits the unsigned `.msixupload` package to Microsoft Store certification;
4. verifies and assembles the public artifacts;
5. generates checksums and GitHub build-provenance attestations;
6. creates exactly one GitHub Release;
7. starts Apple App Store Connect uploads, website deployment, and the Homebrew
   cask update as separate jobs after publication.

The Apple job reuses the published core bundle and uploads iOS and macOS builds
through the protected `apple-appstore` environment. Uploading a build makes it
available to App Store Connect; store review and public availability are separate.

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

Then create and push the matching tag:

```bash
git tag -s v0.2.1 -m "VniDrop 0.2.1"
git push origin v0.2.1
```

The tag must point at the current `origin/master` commit. A failure before the
Create GitHub Release step leaves no GitHub Release, but Play may already contain
a draft and Microsoft Store may already have received a submission. Check those
services before retrying. Play draft reuse requires the same version, configured
track, draft status, and app-signing certificate.

If the Create GitHub Release step itself fails, check whether it created a release
or uploaded partial assets before choosing which jobs to rerun.

A failure after publication leaves the GitHub Release and its assets in place.
Rerun the failed jobs after checking their submission or deployment state.
Rerunning the entire workflow fails preflight because the release already exists.
Keep the existing release and tag when recovering Apple uploads, website
deployment, or the Homebrew update.
