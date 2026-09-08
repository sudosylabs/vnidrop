#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/vnidrop-android-release.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/packaging/android" "$scratch/packaging/version" "$scratch/bin"
cp "$repo_root/packaging/android/"{build-release,verify-apk-signature}.sh "$scratch/packaging/android/"
cp "$repo_root/packaging/version/resolve-version.sh" "$scratch/packaging/version/"
printf 'PRODUCT_VERSION=0.3.3\nRELEASE_CHANNEL=beta\nWINDOWS_VERSION_EPOCH=1\n' > "$scratch/version.properties"
printf 'fixture keystore\n' > "$scratch/upload.jks"

cat > "$scratch/gradlew" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$@" > "$GRADLE_CALLS"
python3 - <<'PY'
import json
import os
from pathlib import Path
from zipfile import ZipFile

outputs = Path("androidApp/build")
for relative, prefix in (("outputs/apk/release/androidApp-release.apk", ""),
                         ("outputs/bundle/release/androidApp-release.aab", "base/")):
    archive = outputs / relative
    archive.parent.mkdir(parents=True, exist_ok=True)
    with ZipFile(archive, "w") as bundle:
        for abi in ("arm64-v8a", "x86_64"):
            for library in ("libvnidrop.so", "libzxingcpp_android.so"):
                entry = f"{prefix}lib/{abi}/{library}"
                if entry != os.environ.get("MISSING_ENTRY"):
                    bundle.writestr(entry, b"native library")
metadata = outputs / "intermediates/merged_manifests/release/processReleaseManifest/output-metadata.json"
metadata.parent.mkdir(parents=True, exist_ok=True)
metadata.write_text(json.dumps({"elements": [{"versionName": "0.3.3", "versionCode": 3003}]}))
PY
SCRIPT
printf '#!/usr/bin/env bash\necho "jar verified."\n' > "$scratch/bin/jarsigner"
printf '#!/usr/bin/env bash\necho "SHA256: 1234"\n' > "$scratch/bin/keytool"
printf '#!/usr/bin/env bash\necho "Signer #1 certificate SHA-256 digest: 1234"\n' > "$scratch/bin/apksigner"
chmod +x "$scratch/gradlew" "$scratch/bin/"* "$scratch/packaging/android/"* "$scratch/packaging/version/"*

export PATH="$scratch/bin:$PATH"
export GRADLE_CALLS="$scratch/gradle-calls"
export GITHUB_REF_TYPE=branch
export VNIDROP_ANDROID_KEYSTORE_PATH="$scratch/upload.jks"
export VNIDROP_ANDROID_KEYSTORE_PASSWORD=fixture
export VNIDROP_ANDROID_KEY_ALIAS=fixture
export VNIDROP_ANDROID_KEY_PASSWORD=fixture
export VNIDROP_ANDROID_UPLOAD_CERT_SHA256=1234
export APKSIGNER="$scratch/bin/apksigner"

bash "$scratch/packaging/android/build-release.sh" >/dev/null
actual_tasks="$(grep '^:' "$GRADLE_CALLS")"
expected_tasks=$':androidApp:lintRelease\n:androidApp:assembleRelease\n:androidApp:bundleRelease'
[[ $actual_tasks == "$expected_tasks" ]] || {
	printf 'Release packaging must lint and build the Release variant: %s\n' "$actual_tasks" >&2
	exit 1
}
(cd "$scratch/build/release/android" && sha256sum --check SHA256SUMS >/dev/null)

for prefix in '' base/; do
	for abi in arm64-v8a x86_64; do
		for library in libvnidrop.so libzxingcpp_android.so; do
			entry="${prefix}lib/$abi/$library"
			if MISSING_ENTRY="$entry" bash "$scratch/packaging/android/build-release.sh" > "$scratch/output" 2>&1; then
				printf 'A release missing %s must fail\n' "$entry" >&2
				exit 1
			fi
			grep -F "Missing or empty Android native library $entry" "$scratch/output" >/dev/null
		done
	done
done
printf 'Android release packaging tests passed.\n'
