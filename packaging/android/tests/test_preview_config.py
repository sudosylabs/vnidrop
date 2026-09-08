"""Verify preview isolation using manifests produced by the actual Android build."""

import os
from pathlib import Path
import subprocess
import unittest
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[3]
MANIFEST = ROOT / "androidApp/build/intermediates/merged_manifests/release/processReleaseManifest/AndroidManifest.xml"
ANDROID = "{http://schemas.android.com/apk/res/android}"


class PreviewConfigTest(unittest.TestCase):
    def generate(self, number=None, succeeds=True):
        command = [str(ROOT / ("gradlew.bat" if os.name == "nt" else "gradlew")),
                   ":androidApp:processReleaseManifest", "--no-daemon", "--no-configuration-cache",
                   "--console=plain", "-Pvnidrop.diagnostics.included=false"]
        if number is not None:
            command.append(f"-Pvnidrop.preview.number={number}")
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(result.returncode == 0, succeeds, result.stdout + result.stderr)
        return ET.parse(MANIFEST).getroot() if succeeds else None

    def test_preview_and_store_manifests_keep_separate_identities(self):
        properties = dict(line.split("=", 1) for line in (ROOT / "version.properties").read_text().splitlines()
                          if "=" in line and not line.startswith("#"))
        version = properties["PRODUCT_VERSION"]
        major, minor, patch = map(int, version.split("."))
        for number, package, name, code in (
            ("17", "com.vnidrop.app.preview", f"{version}-preview.17", 17),
            (None, "com.vnidrop.app", version, major * 1000000 + minor * 1000 + patch),
        ):
            with self.subTest(number=number):
                manifest = self.generate(number)
                self.assertEqual((manifest.get("package"), manifest.get(ANDROID + "versionName"),
                                  manifest.get(ANDROID + "versionCode")), (package, name, str(code)))
                application = manifest.find("application")
                self.assertEqual(application.get(ANDROID + "debuggable", "false"), "false")
                self.assertEqual(application.find("activity").get(ANDROID + "name"), "com.vnidrop.app.MainActivity")
                authorities = [provider.get(ANDROID + "authorities") for provider in application.findall("provider")]
                self.assertIn(f"{package}.fileprovider", authorities)

    def test_invalid_preview_version_fails_configuration(self):
        self.generate("2100000001", succeeds=False)


if __name__ == "__main__":
    unittest.main()
