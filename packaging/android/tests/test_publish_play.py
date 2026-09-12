import importlib.util
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).parents[1] / "publish_play.py"
SPEC = importlib.util.spec_from_file_location("publish_play", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
publish_play = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publish_play)


class PublishPlayTests(unittest.TestCase):
    def test_direct_download_uploads_bundle_without_reading_or_changing_tracks(self):
        for download_only in (True, False):
            with self.subTest(download_only=download_only), tempfile.TemporaryDirectory() as scratch:
                root = Path(scratch)
                bundle = root / "app.aab"
                bundle.write_bytes(b"bundle")
                calls = []

                class FakeClient:
                    def __init__(self, token):
                        self.uploaded = False

                    def request_json(self, method, url, **kwargs):
                        calls.append((method, url))
                        if "/generatedApks/" in url:
                            if not self.uploaded:
                                raise publish_play.PlayApiError(404, "not uploaded")
                            return {"generatedApks": [{"certificateSha256Hash": "aabb",
                                    "generatedUniversalApk": {"downloadId": "apk"}}]}
                        if url.endswith("/edits"):
                            return {"id": "edit"}
                        if "/tracks/" in url:
                            return {"track": "closed-beta", "releases": []}
                        raise AssertionError((method, url))

                    def request(self, method, url, **kwargs):
                        calls.append((method, url))
                        if "/bundles?" in url:
                            self.uploaded = True
                            return b'{"versionCode": 3005}'
                        if "/downloads/" in url:
                            return b"apk"
                        if ":commit?" in url or method == "DELETE":
                            return b""
                        raise AssertionError((method, url))

                args = Namespace(track="" if download_only else "closed-beta", download_only=download_only,
                                 version_code=3005, bundle=bundle, access_token="fixture",
                                 package_name="com.vnidrop.app", expected_app_certificate="aabb",
                                 release_name="0.3.5", apk_output=root / "app.apk", poll_attempts=1, poll_interval=0)
                with patch.object(publish_play, "PlayClient", FakeClient):
                    metadata = publish_play.publish_bundle(args)
                track_calls = [call for call in calls if "/tracks/" in call[1]]
                self.assertEqual([method for method, _ in track_calls], [] if download_only else ["GET", "PUT"])
                self.assertEqual(metadata["track"], None if download_only else "closed-beta")
                self.assertEqual(metadata["releaseStatus"], "unassigned" if download_only else "draft")
                self.assertEqual(args.apk_output.read_bytes(), b"apk")
                self.assertEqual(sum(":commit?" in url for _, url in calls), 1)

    def test_retry_refuses_existing_bundle_with_different_bytes(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            bundle = root / "app.aab"
            bundle.write_bytes(b"new bundle")
            calls = []

            class FakeClient:
                def __init__(self, token):
                    pass

                def request_json(self, method, url, **kwargs):
                    if "/generatedApks/" in url:
                        return {"generatedApks": [{"certificateSha256Hash": "aabb",
                                "generatedUniversalApk": {"downloadId": "apk"}}]}
                    if url.endswith("/edits"):
                        return {"id": "edit"}
                    if url.endswith("/bundles"):
                        return {"bundles": [{"versionCode": 3005, "sha256": "wrong"}]}
                    raise AssertionError((method, url))

                def request(self, method, url, **kwargs):
                    calls.append((method, url))
                    if method != "DELETE":
                        raise AssertionError((method, url))

            args = Namespace(track="", download_only=True, version_code=3005, bundle=bundle,
                             access_token="fixture", package_name="com.vnidrop.app", expected_app_certificate="aabb")
            with patch.object(publish_play, "PlayClient", FakeClient), self.assertRaisesRegex(RuntimeError, "does not match"):
                publish_play.publish_bundle(args)
            self.assertEqual([method for method, _ in calls], ["DELETE"])

    def test_rejects_phone_and_form_factor_production_tracks(self):
        for track in ("production", "wear:production", " Production "):
            with self.subTest(track=track):
                with self.assertRaisesRegex(ValueError, "production"):
                    publish_play.validate_closed_track(track)

    def test_accepts_custom_closed_track(self):
        publish_play.validate_closed_track("closed-beta")

    def test_normalizes_certificate_fingerprint(self):
        self.assertEqual(
            publish_play.normalize_fingerprint("AA:bb 01"),
            "aabb01",
        )

    def test_selects_universal_apk_for_expected_signing_key(self):
        response = {
            "generatedApks": [
                {
                    "certificateSha256Hash": "11:22",
                    "generatedUniversalApk": {"downloadId": "wrong"},
                },
                {
                    "certificateSha256Hash": "AA:BB",
                    "generatedUniversalApk": {"downloadId": "correct"},
                },
            ]
        }
        self.assertEqual(
            publish_play.find_universal_apk(response, "aa:bb"),
            ("aabb", "correct"),
        )

    def test_downloads_generated_apk_as_media(self):
        class FakePlayClient:
            def __init__(self):
                self.download_urls = []
                self.media_attempts = 0

            def request_json(self, method, url):
                self.assert_request(method, url)
                return {
                    "generatedApks": [
                        {
                            "certificateSha256Hash": "AA:BB",
                            "generatedUniversalApk": {
                                "downloadId": "download/id+=",
                            },
                        }
                    ]
                }

            def request(self, method, url):
                self.assert_request(method, url)
                self.download_urls.append(url)
                if not url.endswith("?alt=media"):
                    return b""
                self.media_attempts += 1
                return b"apk" if self.media_attempts == 2 else b""

            @staticmethod
            def assert_request(method, url):
                if method != "GET" or not url.startswith(publish_play.API_ROOT):
                    raise AssertionError(f"unexpected request: {method} {url}")

        client = FakePlayClient()
        with tempfile.TemporaryDirectory() as scratch:
            output = Path(scratch) / "universal.apk"
            fingerprint = publish_play.download_universal_apk(
                client,
                "com.example app",
                2002,
                "aa:bb",
                output,
                attempts=2,
                interval_seconds=0,
            )
            self.assertEqual(fingerprint, "aabb")
            self.assertEqual(output.read_bytes(), b"apk")

        self.assertEqual(
            client.download_urls,
            [
                (
                    f"{publish_play.API_ROOT}/applications/com.example%20app/"
                    "generatedApks/2002/downloads/"
                    "download%2Fid%2B%3D:download?alt=media"
                ),
                (
                    f"{publish_play.API_ROOT}/applications/com.example%20app/"
                    "generatedApks/2002/downloads/"
                    "download%2Fid%2B%3D:download?alt=media"
                ),
            ],
        )

    def test_track_update_preserves_existing_releases_and_adds_draft(self):
        track = {
            "track": "closed-beta",
            "releases": [
                {
                    "name": "0.1.0",
                    "versionCodes": ["1"],
                    "status": "completed",
                }
            ],
        }
        updated = publish_play.build_track_payload(track, 2, "0.2.0")
        self.assertEqual(updated["releases"][0], track["releases"][0])
        self.assertEqual(
            updated["releases"][1],
            {
                "name": "0.2.0",
                "versionCodes": ["2"],
                "status": "draft",
            },
        )

    def test_track_update_rejects_duplicate_version_code(self):
        track = {
            "track": "closed-beta",
            "releases": [{"versionCodes": ["2"], "status": "draft"}],
        }
        with self.assertRaisesRegex(ValueError, "already present"):
            publish_play.build_track_payload(track, 2, "0.2.0")

    def test_finds_existing_release_by_version_code(self):
        expected = {"versionCodes": ["2"], "status": "draft"}
        track = {
            "track": "closed-beta",
            "releases": [
                {"versionCodes": ["1"], "status": "completed"},
                expected,
            ],
        }
        self.assertIs(
            publish_play.find_track_release(track, 2),
            expected,
        )

    def test_returns_none_when_version_is_not_on_track(self):
        track = {
            "track": "closed-beta",
            "releases": [{"versionCodes": ["1"], "status": "completed"}],
        }
        self.assertIsNone(publish_play.find_track_release(track, 2))


if __name__ == "__main__":
    unittest.main()
