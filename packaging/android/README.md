# Android release pipeline

Android releases use two independent credentials:

- the upload keystore signs the APK and AAB;
- a short-lived Google access token publishes the AAB through the Play
  Developer API.

The GitHub release workflow expects these encrypted secrets:

- `ANDROID_UPLOAD_KEYSTORE_BASE64`
- `ANDROID_UPLOAD_KEYSTORE_PASSWORD`
- `ANDROID_UPLOAD_KEY_ALIAS`
- `ANDROID_UPLOAD_KEY_PASSWORD`

It also expects this repository variable:

- `ANDROID_UPLOAD_CERT_SHA256`

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
