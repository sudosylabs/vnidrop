#!/usr/bin/env python3
"""Assemble only verified direct-download packages for a GitHub prerelease."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verify_checksum(payload, checksum):
    if not payload.is_file() or payload.stat().st_size == 0:
        raise ValueError(f"Missing or empty preview package: {payload}")
    entries = [line.split() for line in checksum.read_text().splitlines() if line.strip()]
    matches = [entry[0] for entry in entries
               if len(entry) == 2 and entry[1].lstrip("*") == payload.name]
    if len(matches) != 1 or matches[0].lower() != digest(payload):
        raise ValueError(f"Missing, duplicate, or invalid checksum for {payload.name}")


def assemble(input_dir, output_dir, version, number, commit):
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
        raise ValueError("Invalid product version")
    if not re.fullmatch(r"[1-9][0-9]{0,9}", number) or int(number) > 2100000000:
        raise ValueError("Invalid preview number")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("A full source commit SHA is required")
    if output_dir.exists() and any(output_dir.iterdir()):
        raise ValueError("Preview output directory must be empty")

    label = f"{version}-preview.{number}"
    prefix = f"vnidrop-{version}"
    deb = input_dir / f"{prefix}-linux-deb-x64" / f"vnidrop_{version}-1_amd64.deb"
    rpm = input_dir / f"{prefix}-linux-rpm-x64" / f"vnidrop-{version}-1.x86_64.rpm"
    dmg = input_dir / f"{prefix}-macos-dmg" / f"VniDrop-{version}.dmg"
    exe = input_dir / f"{prefix}-windows-x64" / f"VniDrop_{version}_x64.exe"
    apk = input_dir / f"{prefix}-android-release" / f"VniDrop-{label}.apk"
    packages = [
        (deb, deb.with_name(deb.name + ".sha256"), f"vnidrop_{label}_amd64.deb"),
        (rpm, rpm.with_name(rpm.name + ".sha256"), f"vnidrop-{label}.x86_64.rpm"),
        (dmg, dmg.with_name("preview-dmg.sha256"), f"VniDrop-{label}-arm64.dmg"),
        (exe, exe.with_name("SHA256SUMS"), f"VniDrop-{label}-x64.exe"),
        (apk, apk.with_name("SHA256SUMS"), apk.name),
    ]
    for payload, checksum, _ in packages:
        verify_checksum(payload, checksum)

    apple = json.loads(dmg.with_suffix(".build-info.json").read_text())
    if (apple.get("productVersion"), apple.get("distribution"), apple.get("artifact")) != (
        version, "direct", dmg.name
    ):
        raise ValueError("Unexpected macOS package identity")
    android = json.loads(apk.with_name("android-preview.json").read_text())
    if (android.get("applicationId"), android.get("versionName"), android.get("versionCode"),
        android.get("debuggable")) != ("com.vnidrop.app.preview", label, int(number), False):
        raise ValueError("Unexpected Android preview identity or build configuration")
    if not re.fullmatch(r"[0-9a-f]{64}", android.get("signingCertificateSha256", "")):
        raise ValueError("Android preview signing certificate is missing")

    output_dir.mkdir(parents=True, exist_ok=True)
    files = []
    for payload, _, name in packages:
        shutil.copyfile(payload, output_dir / name)
        files.append({"name": name, "sha256": digest(payload), "bytes": payload.stat().st_size})
    manifest = {
        "productVersion": version,
        "releaseChannel": "preview",
        "previewNumber": int(number),
        "tag": f"preview-{version}-{number}",
        "sourceCommit": commit,
        "android": android,
        "appleDirectBuildNumber": apple["directBuildNumber"],
        "windowsInstallerSigned": False,
        "files": files,
    }
    manifest_path = output_dir / "preview-manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    sums = [f"{item['sha256']}  {item['name']}\n" for item in files]
    sums.append(f"{digest(manifest_path)}  {manifest_path.name}\n")
    (output_dir / "SHA256SUMS").write_text("".join(sums), encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("version", "number", "commit"):
        parser.add_argument(f"--{name}", required=True)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        assemble(args.input, args.output, args.version, args.number, args.commit)
    except (ValueError, OSError, KeyError) as error:
        parser.exit(1, f"Preview assembly failed: {error}\n")
