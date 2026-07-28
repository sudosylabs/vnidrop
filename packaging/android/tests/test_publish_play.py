import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "publish_play.py"
SPEC = importlib.util.spec_from_file_location("publish_play", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
publish_play = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publish_play)


class PublishPlayTests(unittest.TestCase):
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
