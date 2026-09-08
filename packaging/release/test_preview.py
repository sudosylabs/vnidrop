import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location("preview", Path(__file__).with_name("assemble-preview.py"))
preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)


class PreviewAssemblyTest(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        self.inputs = self.root / "inputs"
        self.output = self.root / "output"
        self.payloads = []
        for directory, name, checksum in (
            ("linux-deb-x64", "vnidrop_0.3.3-1_amd64.deb", "vnidrop_0.3.3-1_amd64.deb.sha256"),
            ("linux-rpm-x64", "vnidrop-0.3.3-1.x86_64.rpm", "vnidrop-0.3.3-1.x86_64.rpm.sha256"),
            ("macos-dmg", "VniDrop-0.3.3.dmg", "preview-dmg.sha256"),
            ("windows-x64", "VniDrop_0.3.3_x64.exe", "SHA256SUMS"),
            ("android-release", "VniDrop-0.3.3-preview.17.apk", "SHA256SUMS"),
        ):
            folder = self.inputs / f"vnidrop-0.3.3-{directory}"
            folder.mkdir(parents=True)
            payload = folder / name
            payload.write_bytes(name.encode())
            self.payloads.append(payload)
            (folder / checksum).write_text(f"{preview.digest(payload)}  {name}\n")
        apple = self.inputs / "vnidrop-0.3.3-macos-dmg"
        (apple / "VniDrop-0.3.3.build-info.json").write_text(json.dumps({
            "productVersion": "0.3.3", "distribution": "direct", "artifact": "VniDrop-0.3.3.dmg",
            "directBuildNumber": "20260908.1200.00",
        }))
        self.android_metadata = self.inputs / "vnidrop-0.3.3-android-release/android-preview.json"
        self.android_metadata.write_text(json.dumps({
            "applicationId": "com.vnidrop.app.preview", "versionName": "0.3.3-preview.17",
            "versionCode": 17, "debuggable": False, "signingCertificateSha256": "a" * 64,
        }))

    def assemble(self, **overrides):
        arguments = dict(input_dir=self.inputs, output_dir=self.output,
                         version="0.3.3", number="17", commit="a" * 40)
        arguments.update(overrides)
        preview.assemble(**arguments)

    def test_publishes_only_preview_downloads_and_verifiable_manifest(self):
        (self.payloads[2].parent / "appcast.xml").write_text("stable feed must not ship")
        (self.payloads[3].parent / "submission.msixupload").write_text("store package must not ship")
        self.assemble()
        names = {path.name for path in self.output.iterdir()}
        self.assertEqual(names, {
            "vnidrop_0.3.3-preview.17_amd64.deb", "vnidrop-0.3.3-preview.17.x86_64.rpm",
            "VniDrop-0.3.3-preview.17-arm64.dmg", "VniDrop-0.3.3-preview.17-x64.exe",
            "VniDrop-0.3.3-preview.17.apk", "preview-manifest.json", "SHA256SUMS",
        })
        for name in names - {"SHA256SUMS"}:
            preview.verify_checksum(self.output / name, self.output / "SHA256SUMS")
        manifest = json.loads((self.output / "preview-manifest.json").read_text())
        self.assertEqual((manifest["tag"], manifest["releaseChannel"], manifest["sourceCommit"]),
                         ("preview-0.3.3-17", "preview", "a" * 40))

    def test_corrupt_or_missing_platform_prevents_any_publication_assets(self):
        for payload in self.payloads:
            with self.subTest(payload=payload.name):
                original = payload.read_bytes()
                for content in (b"tampered", b""):
                    payload.write_bytes(content)
                    with self.assertRaises(ValueError):
                        self.assemble()
                    self.assertFalse(self.output.exists())
                payload.unlink()
                with self.assertRaises(ValueError):
                    self.assemble()
                payload.write_bytes(original)

    def test_store_identity_debug_build_and_wrong_version_are_rejected(self):
        original = json.loads(self.android_metadata.read_text())
        for field, value in (("applicationId", "com.vnidrop.app"), ("debuggable", True),
                             ("versionName", "0.3.3"), ("versionCode", 3003),
                             ("signingCertificateSha256", "")):
            with self.subTest(field=field):
                self.android_metadata.write_text(json.dumps({**original, field: value}))
                with self.assertRaises(ValueError):
                    self.assemble()
                self.assertFalse(self.output.exists())

    def test_refuses_overwriting_existing_downloads_and_invalid_identity(self):
        for overrides in ({"number": "0"}, {"number": "2100000001"}, {"number": "../17"},
                          {"version": "0.3.3-preview.17"}, {"commit": "master"}):
            with self.subTest(overrides=overrides), self.assertRaises(ValueError):
                self.assemble(**overrides)
        self.assemble()
        with self.assertRaisesRegex(ValueError, "must be empty"):
            self.assemble()


if __name__ == "__main__":
    unittest.main()
