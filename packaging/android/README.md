# Android release pipeline

This guide describes the normal Play-connected release. For direct testing,
the [preview workflow](../release/PREVIEW.md) builds a Release APK without an
AAB or Play submission. It uses a dedicated preview signing key and the separate
`com.vnidrop.app.preview` application ID. Its one-time signing setup and version
rules are documented in that guide; do not use the Play upload key for previews.

Android signing and publishing use separate credentials:

- the upload keystore signs the APK and AAB;
- a short-lived Google access token publishes the AAB through the Play
  Developer API.

The GitHub release workflow expects these encrypted secrets:

- `ANDROID_UPLOAD_KEYSTORE_BASE64`
- `ANDROID_UPLOAD_KEYSTORE_PASSWORD`
- `ANDROID_UPLOAD_KEY_ALIAS`
- `ANDROID_UPLOAD_KEY_PASSWORD`
- `VNIDROP_DIAGNOSTICS_INGEST_KEY`

It also expects these repository variables:

- `ANDROID_UPLOAD_CERT_SHA256`
- `VNIDROP_DIAGNOSTICS_ENDPOINT`

The protected `play-closed-testing` GitHub Environment supplies:

- `PLAY_APP_SIGNING_CERT_SHA256`
- `GCP_WORKLOAD_IDENTITY_PROVIDER`
- `GCP_PLAY_SERVICE_ACCOUNT`
- `PLAY_PACKAGE_NAME` (`com.vnidrop.app`)
- `PLAY_CLOSED_TRACK` (the existing closed-test track identifier)

The upload and app-signing certificate fingerprints are public identifiers from
Play Console's App signing page. Do not store a private key in a repository
variable.

`packaging/android/build-release.sh` creates an upload-signed AAB and APK,
verifies their canonical version and upload certificate, and writes checksums.
The release workflow uploads only the AAB to Play. It then downloads the
universal APK generated and signed by Play for the public GitHub Release.

Official release builds require working bug-report configuration. The build reads
`VNIDROP_DIAGNOSTICS_ENDPOINT` and `VNIDROP_DIAGNOSTICS_INGEST_KEY` from the
workflow environment, or the equivalent `vnidrop.diagnostics.endpoint` and
`vnidrop.diagnostics.ingestKey` user-level Gradle properties for local builds.
Missing configuration fails the release build. Reports are sent only when the
user submits the form.

Play publishing is deliberately restricted to `draft` releases on
`PLAY_CLOSED_TRACK`. Production promotion is not part of this pipeline.

## One-time setup

1. In Play Console, link a Google Cloud project and grant the deployment
   service account permission to manage releases for VniDrop.
2. In Google Cloud, enable the Google Play Android Developer API and configure
   a Workload Identity Federation provider that trusts this repository's
   GitHub Actions identity. Permit the service account to receive federated
   tokens from that provider.
3. Create the `play-closed-testing` GitHub Environment. Add the five variables
   listed above and restrict deployment branches/tags to the release policy.
4. Add the four upload-keystore secrets and
   `ANDROID_UPLOAD_CERT_SHA256` in the repository settings.

No Google service-account JSON key is stored in GitHub. The workflow exchanges
GitHub's OIDC identity for a short-lived Google access token.

Build-configuration regression checks (synthetic credentials; no reports sent):

```bash
python3 packaging/android/tests/test_diagnostics_config.py
```
