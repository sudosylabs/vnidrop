#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any


API_ROOT = "https://androidpublisher.googleapis.com/androidpublisher/v3"
UPLOAD_ROOT = "https://androidpublisher.googleapis.com/upload/androidpublisher/v3"
RETRYABLE_STATUS = {429, 500, 502, 503, 504}


class PlayApiError(RuntimeError):
    def __init__(self, status: int | None, message: str) -> None:
        super().__init__(message)
        self.status = status


class PlayClient:
    def __init__(self, token: str) -> None:
        if not token:
            raise ValueError("Google Play access token is required")
        self.token = token

    def request(
        self,
        method: str,
        url: str,
        *,
        body: bytes | None = None,
        content_type: str | None = None,
        timeout: int = 180,
        attempts: int = 5,
    ) -> bytes:
        headers = {
            "Authorization": f"Bearer {self.token}",
            "Accept": "application/json",
        }
        if content_type is not None:
            headers["Content-Type"] = content_type

        for attempt in range(1, attempts + 1):
            request = urllib.request.Request(
                url,
                data=body,
                headers=headers,
                method=method,
            )
            try:
                with urllib.request.urlopen(request, timeout=timeout) as response:
                    return response.read()
            except urllib.error.HTTPError as error:
                error_body = error.read().decode("utf-8", errors="replace")
                if error.code not in RETRYABLE_STATUS or attempt == attempts:
                    raise PlayApiError(
                        error.code,
                        f"Google Play API returned HTTP {error.code}: {error_body}",
                    ) from error
            except urllib.error.URLError as error:
                if attempt == attempts:
                    raise PlayApiError(
                        None,
                        f"Google Play API request failed: {error.reason}",
                    ) from error
            time.sleep(2 ** (attempt - 1))
        raise AssertionError("request retry loop exited unexpectedly")

    def request_json(
        self,
        method: str,
        url: str,
        *,
        value: Any | None = None,
        timeout: int = 180,
    ) -> dict[str, Any]:
        body = None
        content_type = None
        if value is not None:
            body = json.dumps(value, separators=(",", ":")).encode()
            content_type = "application/json"
        response = self.request(
            method,
            url,
            body=body,
            content_type=content_type,
            timeout=timeout,
        )
        return json.loads(response) if response else {}


def normalize_fingerprint(value: str) -> str:
    return "".join(character for character in value.lower() if character.isalnum())


def validate_closed_track(track: str) -> None:
    normalized = track.strip().casefold()
    if not normalized:
        raise ValueError("Play track is required")
    if normalized == "production" or normalized.endswith(":production"):
        raise ValueError(
            "production tracks are forbidden by this closed-testing pipeline"
        )


def find_universal_apk(
    response: dict[str, Any],
    expected_fingerprint: str,
) -> tuple[str, str] | None:
    expected = normalize_fingerprint(expected_fingerprint)
    for signing_key in response.get("generatedApks", []):
        fingerprint = normalize_fingerprint(
            str(signing_key.get("certificateSha256Hash", ""))
        )
        if fingerprint != expected:
            continue
        universal = signing_key.get("generatedUniversalApk") or {}
        download_id = universal.get("downloadId")
        if download_id:
            return fingerprint, str(download_id)
    return None


def build_track_payload(
    track: dict[str, Any],
    version_code: int,
    release_name: str,
) -> dict[str, Any]:
    releases = list(track.get("releases") or [])
    expected_code = str(version_code)
    if any(
        expected_code in [str(code) for code in release.get("versionCodes", [])]
        for release in releases
    ):
        raise ValueError(
            f"version code {version_code} is already present in track "
            f"{track.get('track', '<unknown>')}"
        )
    releases.append(
        {
            "name": release_name,
            "versionCodes": [expected_code],
            "status": "draft",
        }
    )
    return {"track": track["track"], "releases": releases}


def find_track_release(
    track: dict[str, Any],
    version_code: int,
) -> dict[str, Any] | None:
    expected_code = str(version_code)
    return next(
        (
            release
            for release in track.get("releases") or []
            if expected_code
            in [str(code) for code in release.get("versionCodes", [])]
        ),
        None,
    )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def generated_apks_url(package_name: str, version_code: int) -> str:
    package = urllib.parse.quote(package_name, safe="")
    return f"{API_ROOT}/applications/{package}/generatedApks/{version_code}"


def get_generated_apks(
    client: PlayClient,
    package_name: str,
    version_code: int,
) -> dict[str, Any] | None:
    try:
        return client.request_json(
            "GET",
            generated_apks_url(package_name, version_code),
        )
    except PlayApiError as error:
        if error.status == 404:
            return None
        raise


def download_universal_apk(
    client: PlayClient,
    package_name: str,
    version_code: int,
    expected_fingerprint: str,
    output: Path,
    *,
    attempts: int,
    interval_seconds: int,
) -> str:
    for attempt in range(1, attempts + 1):
        response = get_generated_apks(client, package_name, version_code)
        if response is not None:
            selected = find_universal_apk(response, expected_fingerprint)
            if selected is not None:
                fingerprint, download_id = selected
                package = urllib.parse.quote(package_name, safe="")
                download = urllib.parse.quote(download_id, safe="")
                url = (
                    f"{API_ROOT}/applications/{package}/generatedApks/"
                    f"{version_code}/downloads/{download}:download"
                )
                output.parent.mkdir(parents=True, exist_ok=True)
                output.write_bytes(client.request("GET", url))
                if output.stat().st_size == 0:
                    raise RuntimeError("Google Play returned an empty universal APK")
                return fingerprint
        if attempt < attempts:
            time.sleep(interval_seconds)
    raise RuntimeError(
        "Google Play did not provide a universal APK signed with the expected "
        f"certificate after {attempts} attempts"
    )


def publish_bundle(args: argparse.Namespace) -> dict[str, Any]:
    validate_closed_track(args.track)
    if args.version_code < 1:
        raise ValueError("version code must be positive")
    if not args.bundle.is_file() or args.bundle.stat().st_size == 0:
        raise ValueError(f"AAB is missing or empty: {args.bundle}")

    client = PlayClient(args.access_token)
    existing = get_generated_apks(client, args.package_name, args.version_code)
    if existing is not None:
        selected = find_universal_apk(existing, args.expected_app_certificate)
        if selected is None:
            raise RuntimeError(
                "version code already exists in Play, but no universal APK matches "
                "the expected app-signing certificate"
            )
        package = urllib.parse.quote(args.package_name, safe="")
        edit = client.request_json(
            "POST",
            f"{API_ROOT}/applications/{package}/edits",
            value={},
        )
        edit_id = str(edit["id"])
        edit_base = f"{API_ROOT}/applications/{package}/edits/{edit_id}"
        try:
            track_id = urllib.parse.quote(args.track, safe="")
            track = client.request_json(
                "GET",
                f"{edit_base}/tracks/{track_id}",
            )
            release = find_track_release(track, args.version_code)
            if release is None or release.get("status") != "draft":
                raise RuntimeError(
                    "version code already exists in Play but is not a draft on "
                    f"the configured track {args.track}"
                )
        finally:
            try:
                client.request("DELETE", edit_base, attempts=1)
            except PlayApiError:
                pass
        source = "existing"
    else:
        package = urllib.parse.quote(args.package_name, safe="")
        edit = client.request_json(
            "POST",
            f"{API_ROOT}/applications/{package}/edits",
            value={},
        )
        edit_id = str(edit["id"])
        committed = False
        edit_base = f"{API_ROOT}/applications/{package}/edits/{edit_id}"
        try:
            upload_url = (
                f"{UPLOAD_ROOT}/applications/{package}/edits/{edit_id}/bundles"
                "?uploadType=media"
            )
            try:
                uploaded = json.loads(
                    client.request(
                        "POST",
                        upload_url,
                        body=args.bundle.read_bytes(),
                        content_type="application/octet-stream",
                        attempts=1,
                    )
                )
            except PlayApiError:
                bundles = client.request_json("GET", f"{edit_base}/bundles")
                matches = [
                    bundle
                    for bundle in bundles.get("bundles", [])
                    if int(bundle.get("versionCode", 0)) == args.version_code
                ]
                if len(matches) != 1:
                    raise
                uploaded = matches[0]

            uploaded_code = int(uploaded["versionCode"])
            if uploaded_code != args.version_code:
                raise RuntimeError(
                    f"Play accepted version code {uploaded_code}, expected "
                    f"{args.version_code}"
                )

            track_id = urllib.parse.quote(args.track, safe="")
            track_url = f"{edit_base}/tracks/{track_id}"
            track = client.request_json("GET", track_url)
            payload = build_track_payload(track, args.version_code, args.release_name)
            client.request_json("PUT", track_url, value=payload)
            commit_url = (
                f"{edit_base}:commit"
                "?changesInReviewBehavior=ERROR_IF_IN_REVIEW"
            )
            client.request("POST", commit_url, body=b"")
            committed = True
            source = "uploaded"
        finally:
            if not committed:
                try:
                    client.request("DELETE", edit_base, attempts=1)
                except PlayApiError:
                    pass

    fingerprint = download_universal_apk(
        client,
        args.package_name,
        args.version_code,
        args.expected_app_certificate,
        args.apk_output,
        attempts=args.poll_attempts,
        interval_seconds=args.poll_interval,
    )
    return {
        "packageName": args.package_name,
        "track": args.track,
        "releaseStatus": "draft",
        "releaseName": args.release_name,
        "versionCode": args.version_code,
        "bundleSha256": sha256_file(args.bundle),
        "universalApk": args.apk_output.name,
        "universalApkSha256": sha256_file(args.apk_output),
        "appSigningCertificateSha256": fingerprint,
        "source": source,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Stage a signed AAB on a closed Play track and download Play's "
            "app-signed universal APK"
        )
    )
    parser.add_argument(
        "--access-token",
        default=os.environ.get("GOOGLE_PLAY_ACCESS_TOKEN"),
    )
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--package-name", required=True)
    parser.add_argument("--track", required=True)
    parser.add_argument("--version-code", type=int, required=True)
    parser.add_argument("--release-name", required=True)
    parser.add_argument("--expected-app-certificate", required=True)
    parser.add_argument("--apk-output", type=Path, required=True)
    parser.add_argument("--metadata-output", type=Path, required=True)
    parser.add_argument("--poll-attempts", type=int, default=18)
    parser.add_argument("--poll-interval", type=int, default=10)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        metadata = publish_bundle(args)
    except (KeyError, ValueError, RuntimeError, PlayApiError) as error:
        print(f"Play closed-testing publication failed: {error}", file=sys.stderr)
        return 1
    args.metadata_output.parent.mkdir(parents=True, exist_ok=True)
    args.metadata_output.write_text(
        json.dumps(metadata, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        f"Staged {args.release_name} ({args.version_code}) as a draft on "
        f"{args.track}"
    )
    print(f"Downloaded Play-signed APK: {args.apk_output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
