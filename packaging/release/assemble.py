#!/usr/bin/env python3
"""Verify selected release packages and retain links to unchanged platforms."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil


PLATFORMS = ("windows", "linux", "android", "macos")
SUFFIXES = {"windows": (".exe",), "linux": (".deb", ".rpm"),
            "android": ("-play-universal.apk",), "macos": (".dmg",)}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verify_checksum(payload, checksum):
    if not payload.is_file() or not payload.stat().st_size:
        raise ValueError(f"Missing or empty package: {payload}")
    entries = [line.split() for line in checksum.read_text(encoding="utf-8-sig").splitlines() if line.strip()]
    matches = [entry[0].lower() for entry in entries if len(entry) == 2 and entry[1].lstrip("*") == payload.name]
    if matches != [digest(payload)]:
        raise ValueError(f"Missing, duplicate, or invalid checksum for {payload.name}")


def version_tuple(version):
    if not isinstance(version, str) or not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", version):
        raise ValueError("Invalid release version")
    return tuple(map(int, version.split(".")))


def download_index(manifest):
    version_tuple(manifest["productVersion"])
    if manifest.get("releaseChannel") == "preview" or manifest["tag"] != "v" + manifest["productVersion"]:
        raise ValueError("Expected a normal release manifest")
    downloads = manifest.get("downloads")
    if downloads is None:
        downloads = {}
        for platform, suffixes in SUFFIXES.items():
            files = [f for f in manifest["files"] if f["name"].endswith(suffixes)]
            if files:
                downloads[platform] = {"version": manifest["productVersion"], "tag": manifest["tag"], "files": files}
    if not isinstance(downloads, dict) or set(downloads) - set(PLATFORMS):
        raise ValueError("Invalid download platforms")
    for platform, entry in downloads.items():
        if version_tuple(entry["version"]) > version_tuple(manifest["productVersion"]) or entry["tag"] != "v" + entry["version"]:
            raise ValueError("Invalid download version or tag")
        files = entry["files"]
        if len(files) != len(SUFFIXES[platform]) or any(sum(f["name"].endswith(suffix) for f in files) != 1 for suffix in SUFFIXES[platform]):
            raise ValueError(f"Incomplete or duplicate downloads for {platform}")
        for file in files:
            if (Path(file["name"]).name != file["name"] or "/" in file["name"] or "\\" in file["name"]
                    or entry["version"] not in file["name"] or not re.fullmatch(r"[a-f0-9]{64}", file["sha256"])
                    or not isinstance(file["bytes"], int) or file["bytes"] <= 0):
                raise ValueError("Invalid download file metadata")
    return downloads


def assemble(input_dir, output_dir, version, tag, commit, android_code, windows_package,
             channel="beta", platforms="windows,linux,android,macos", stores="windows,android,macos,ios", previous=None):
    version_tuple(version)
    if tag != "v" + version:
        raise ValueError("Release tag does not match version")
    selected = platforms.split(",")
    if not selected or len(set(selected)) != len(selected) or set(selected) - set(PLATFORMS):
        raise ValueError("Select valid direct-download platforms")
    store_set = set(stores.split(",")) if stores else set()
    if store_set - {"windows", "android", "macos", "ios"}:
        raise ValueError("Invalid store platforms")
    if output_dir.exists() and any(output_dir.iterdir()):
        raise ValueError("Release output directory must be empty")
    downloads = {}
    previous_manifest = None
    if previous and previous.exists():
        previous_manifest = json.loads(previous.read_text(encoding="utf-8-sig"))
        downloads = download_index(previous_manifest)
        if version_tuple(previous_manifest["productVersion"]) >= version_tuple(version):
            raise ValueError("A new release must advance the published product version")
    payloads = []
    manifest = {"productVersion": version, "releaseChannel": channel, "tag": tag,
                "sourceCommit": commit, "platforms": selected, "platformVersions": {}}

    def package(folder, name, checksum=None):
        path = input_dir / folder / name
        if not path.is_file() or not path.stat().st_size:
            raise ValueError(f"Missing or empty package: {path}")
        if checksum:
            verify_checksum(path, path.with_name(checksum))
        payloads.append(path)
        return path

    if "linux" in selected:
        for folder, name in (("deb", f"vnidrop_{version}-1_amd64.deb"), ("rpm", f"vnidrop-{version}-1.x86_64.rpm")):
            package(folder, name, name + ".sha256")
    if "macos" in selected:
        dmg = package("macos", f"VniDrop-{version}.dmg")
        appcast = package("macos", "appcast.xml")
        package("macos", f"VnidropCore-{version}.zip", f"VnidropCore-{version}.zip.sha256")
        apple = json.loads(dmg.with_suffix(".build-info.json").read_text())
        if ((apple["productVersion"], apple["distribution"], apple["artifact"]) != (version, "direct", dmg.name)
                or not re.fullmatch(r"[1-9][0-9]*(\.[0-9]+){0,2}", apple["directBuildNumber"])
                or dmg.name not in appcast.read_text()):
            raise ValueError("Invalid macOS package metadata")
        manifest["platformVersions"]["appleDirectBuildNumber"] = apple["directBuildNumber"]
    elif previous_manifest:
        # Installed Macs still request /releases/latest/download/appcast.xml.
        feeds = [f for f in previous_manifest["files"] if f["name"] == "appcast.xml"]
        if "macos" in downloads and len(feeds) != 1:
            raise ValueError("Previous macOS download has no update feed")
        if feeds:
            feed = previous.with_name("appcast.xml")
            if len(feeds) != 1 or digest(feed) != feeds[0]["sha256"] or feed.stat().st_size != feeds[0]["bytes"]:
                raise ValueError("Previous update feed checksum mismatch")
            payloads.append(feed)
    if "android" in selected:
        apk = package("play", f"VniDrop-{version}-{android_code}-play-universal.apk", "SHA256SUMS")
        metadata = apk.with_name("play-release.json")
        verify_checksum(metadata, apk.with_name("SHA256SUMS"))
        play = json.loads(metadata.read_text())
        expected_status = "draft" if "android" in store_set else "unassigned"
        if (play["releaseName"], play["versionCode"], play["releaseStatus"]) != (version, int(android_code), expected_status):
            raise ValueError("Invalid Play package identity or destination")
        track = play["track"]
        if "android" in store_set and (not track or track.lower().strip() == "production" or track.lower().strip().endswith(":production")):
            raise ValueError("Production Play tracks are forbidden")
        if "android" not in store_set and track is not None:
            raise ValueError("Direct-only Android must not select a store track")
        manifest["platformVersions"]["androidVersionCode"] = int(android_code)
        manifest["play"] = {"track": track, "status": expected_status, "bundleSha256": play["bundleSha256"],
                            "appSigningCertificateSha256": play["appSigningCertificateSha256"]}
    if "windows" in selected:
        exe = package("windows", f"VniDrop_{version}_x64.exe", "SHA256SUMS")
        metadata = exe.with_suffix(".build-info.json")
        verify_checksum(metadata, exe.with_name("SHA256SUMS"))
        windows = json.loads(metadata.read_text(encoding="utf-8-sig"))
        installer = windows["directInstaller"]
        if ((windows["appVersion"], windows["packageVersion"], installer["artifact"]) != (version, windows_package, exe.name)
                or installer["unsigned"] is not True or installer["smartScreenWarningExpected"] is not True):
            raise ValueError("Invalid Windows package metadata")
        manifest["platformVersions"]["windowsPackageVersion"] = windows_package
        manifest["windowsDirect"] = {"publicReleaseAsset": True, "installer": exe.name, "sha256": digest(exe),
                                     "unsigned": True, "smartScreenWarningExpected": True}
        if "windows" in store_set:
            for suffix in ("msix", "msixupload"):
                path = exe.with_suffix("." + suffix)
                verify_checksum(path, exe.with_name("SHA256SUMS"))
            manifest["windowsStore"] = {"publicReleaseAsset": False, "msixUpload": path.name, "sha256": digest(path)}
    manifest["files"] = [{"name": p.name, "sha256": digest(p), "bytes": p.stat().st_size} for p in payloads]
    for platform in selected:
        downloads[platform] = {"version": version, "tag": tag,
                               "files": [f for f in manifest["files"] if f["name"].endswith(SUFFIXES[platform])]}
    manifest["downloads"] = downloads
    download_index(manifest)
    output_dir.mkdir(parents=True, exist_ok=True)
    for payload in payloads:
        shutil.copyfile(payload, output_dir / payload.name)
    manifest_path = output_dir / "release-manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
    sums = [f"{digest(path)}  {path.name}\n" for path in sorted(output_dir.iterdir())]
    (output_dir / "SHA256SUMS").write_text("".join(sums), encoding="utf-8", newline="\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("version", "tag", "commit", "android-code", "windows-package", "channel", "platforms", "stores"):
        parser.add_argument("--" + name, required=True)
    for name in ("input-dir", "output-dir"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--previous", type=Path)
    try:
        assemble(**vars(parser.parse_args()))
    except (ValueError, KeyError, OSError) as error:
        parser.exit(1, f"Release assembly failed: {error}\n")
