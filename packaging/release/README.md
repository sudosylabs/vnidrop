# Coordinated releases

Only `.github/workflows/release.yml` responds to version tags. It verifies that
the tag matches `version.properties` and points at the current `master`, then
calls the native platform workflows in parallel.

The tag workflow runs only when the repository variable
`RELEASE_PIPELINE_ENABLED` is exactly `true`. Leave it unset or set it to
`false` to disable all coordinated releases, including Play uploads, without
disabling release validation on pull requests.

Platform workflows upload private workflow artifacts. After every native build
passes, the release pipeline:

1. stages the signed AAB as a draft on the configured Play closed-test track;
2. submits the unsigned `.msixupload` package to Microsoft Store certification;
3. downloads the universal APK signed by Play;
4. verifies and assembles the public artifacts;
5. generates checksums and GitHub build-provenance attestations;
6. creates exactly one GitHub Release;
7. updates the Homebrew cask.

Public GitHub Release assets are the DEB, RPM, notarized DMG, Sparkle appcast,
Play-signed universal APK, unsigned Windows direct installer, checksum file,
and release manifest.

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

The tag must point at the current `origin/master` commit. A failed run creates
no GitHub Release; a rerun safely reuses an already-staged Play draft only when
the version, configured track, draft status, and app-signing certificate all
match.
