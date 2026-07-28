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
2. downloads the universal APK signed by Play;
3. verifies and assembles the public artifacts;
4. generates checksums and GitHub build-provenance attestations;
5. creates exactly one GitHub Release;
6. updates the Homebrew cask.

Public GitHub Release assets are the DEB, RPM, notarized DMG, Sparkle appcast,
Play-signed universal APK, checksum file, and release manifest.

The unsigned Microsoft `.msixupload` and upload-signed Android AAB remain
private workflow artifacts. Partner Center submission stays manual until the
first Microsoft Store release is certified. The Play release remains a draft
on a closed-testing track; this pipeline cannot publish to production.

To release, first update and merge `version.properties`, including a monotonic
Android version code. Apple Store and Direct build numbers are derived
independently at build time. Then create and push the matching tag:

```bash
git tag -s v0.2.0 -m "VniDrop 0.2.0"
git push origin v0.2.0
```

The tag must point at the current `origin/master` commit. A failed run creates
no GitHub Release; a rerun safely reuses an already-staged Play draft only when
the version, configured track, draft status, and app-signing certificate all
match.
