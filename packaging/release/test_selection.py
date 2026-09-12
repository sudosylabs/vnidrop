import importlib.util
import itertools
import json
from pathlib import Path
import shutil
import tempfile
import unittest


def module(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


planner = module("plan")
release = module("assemble")


class SelectionTest(unittest.TestCase):
    def test_release_ordering_searches_all_pages_and_ignores_previews_and_drafts(self):
        normal = {"tag_name": "v0.3.4", "draft": False, "prerelease": False}
        pages = [[{**normal, "tag_name": "preview-0.3.4-100", "prerelease": True},
                  {**normal, "tag_name": "v0.3.8", "draft": True}], [normal]]
        self.assertEqual(planner.previous_release(pages, "0.3.5"), "v0.3.4")
        self.assertEqual(planner.previous_release([[]], "0.3.5"), "")
        for version in ("0.3.3", "0.3.4"):
            with self.assertRaisesRegex(ValueError, "newer"):
                planner.previous_release(pages, version)

    def test_every_platform_and_distribution_combination(self):
        for flags in itertools.product((False, True), repeat=5):
            selected = dict(zip(planner.PLATFORMS, flags))
            for distribution in ("direct", "stores", "both"):
                with self.subTest(selected=selected, distribution=distribution):
                    direct = {p for p, enabled in selected.items() if enabled and p != "ios" and distribution != "stores"}
                    stores = {p for p, enabled in selected.items() if enabled and p != "linux" and distribution != "direct"}
                    if not direct and not stores:
                        with self.assertRaises(ValueError):
                            planner.plan({**selected, "distribution": distribution})
                        continue
                    result = planner.plan({**selected, "distribution": distribution})
                    self.assertEqual(set(filter(None, result["direct_platforms"].split(","))), direct)
                    self.assertEqual(set(filter(None, result["store_platforms"].split(","))), stores)
                    self.assertEqual(result["macos"], "macos" in direct)
                    self.assertEqual(result["apple_store"], bool(stores & {"ios", "macos"}))
                    self.assertEqual(result["windows_store"], "windows" in stores)
                    self.assertEqual(result["android_store"], "android" in stores)

    def test_windows_direct_never_builds_apple(self):
        result = planner.plan({"windows": True, "distribution": "direct"})
        self.assertEqual(result, {
            "direct_platforms": "windows", "store_platforms": "", "windows": True,
            "linux": False, "android": False, "macos": False, "windows_store": False,
            "android_store": False, "apple_store": False, "apple_platforms": "",
        })

    def test_preview_cannot_submit_to_stores(self):
        result = planner.plan({p: True for p in planner.PLATFORMS}, preview=True)
        self.assertFalse(result["store_platforms"])
        self.assertFalse(result["apple_store"])

    def test_workflow_distribution_labels_match_the_internal_plan(self):
        for label, value in (("Direct downloads + stores", "both"), ("Direct downloads", "direct"), ("Stores", "stores")):
            selected = {p: True for p in planner.PLATFORMS}
            self.assertEqual(planner.plan({**selected, "distribution": label}), planner.plan({**selected, "distribution": value}))

    def test_invalid_inputs_fail(self):
        for inputs in ({"windows": "false"}, {"windows": True, "distribution": "unknown"}):
            with self.assertRaises(ValueError):
                planner.plan(inputs)


class ReleaseAssemblyTest(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        self.inputs = self.root / "input"
        self.output = self.root / "output"
        self.write("deb", "vnidrop_0.3.5-1_amd64.deb", b"deb", "vnidrop_0.3.5-1_amd64.deb.sha256")
        self.write("rpm", "vnidrop-0.3.5-1.x86_64.rpm", b"rpm", "vnidrop-0.3.5-1.x86_64.rpm.sha256")
        self.exe = self.write("windows", "VniDrop_0.3.5_x64.exe", b"exe", "SHA256SUMS")
        self.write("windows", "VniDrop_0.3.5_x64.build-info.json", json.dumps({
            "appVersion": "0.3.5", "packageVersion": "1.3.5.0",
            "directInstaller": {"artifact": self.exe.name, "unsigned": True, "smartScreenWarningExpected": True},
        }).encode(), "SHA256SUMS")
        self.write("macos", "VniDrop-0.3.5.dmg", b"dmg")
        self.write("macos", "appcast.xml", b"VniDrop-0.3.5.dmg")
        self.write("macos", "VnidropCore-0.3.5.zip", b"core", "VnidropCore-0.3.5.zip.sha256")
        self.write("macos", "VniDrop-0.3.5.build-info.json", json.dumps({
            "productVersion": "0.3.5", "distribution": "direct", "artifact": "VniDrop-0.3.5.dmg",
            "directBuildNumber": "20260912.1200.00",
        }).encode())
        self.write("play", "VniDrop-0.3.5-3005-play-universal.apk", b"apk", "SHA256SUMS")
        self.write("play", "play-release.json", json.dumps({
            "releaseName": "0.3.5", "versionCode": 3005, "releaseStatus": "unassigned", "track": None,
            "bundleSha256": "a" * 64, "appSigningCertificateSha256": "b" * 64,
        }).encode(), "SHA256SUMS")

    def write(self, folder, name, content, checksum=None):
        directory = self.inputs / folder
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / name
        path.write_bytes(content)
        if checksum:
            with (directory / checksum).open("a") as stream:
                stream.write(f"{release.digest(path)}  {name}\n")
        return path

    def assemble(self, **overrides):
        args = dict(input_dir=self.inputs, output_dir=self.output, version="0.3.5", tag="v0.3.5",
                    commit="a" * 40, android_code="3005", windows_package="1.3.5.0", stores="",
                    platforms="windows,linux")
        args.update(overrides)
        release.assemble(**args)
        return json.loads((self.output / "release-manifest.json").read_text())

    def test_each_nonempty_subset_publishes_only_its_packages(self):
        for count in range(1, 5):
            for selected in itertools.combinations(release.PLATFORMS, count):
                with self.subTest(selected=selected):
                    manifest = self.assemble(platforms=",".join(selected))
                    self.assertEqual(set(manifest["downloads"]), set(selected))
                    self.assertEqual(set(manifest["platforms"]), set(selected))
                    for path in self.output.iterdir():
                        if path.name != "SHA256SUMS":
                            release.verify_checksum(path, self.output / "SHA256SUMS")
                    self.assertEqual(self.exe.name in [f["name"] for f in manifest["files"]], "windows" in selected)
                    self.assertEqual("appcast.xml" in [f["name"] for f in manifest["files"]], "macos" in selected)
                    shutil.rmtree(self.output)

    def test_skipped_platform_artifacts_are_not_required(self):
        for folder in ("macos", "play"):
            shutil.rmtree(self.inputs / folder)
        self.assertEqual(self.assemble()["platforms"], ["windows", "linux"])

    def test_selected_corruption_or_missing_file_fails_before_output(self):
        self.exe.write_bytes(b"tampered")
        with self.assertRaisesRegex(ValueError, "checksum"):
            self.assemble()
        self.assertFalse(self.output.exists())
        self.exe.unlink()
        with self.assertRaisesRegex(ValueError, "Missing"):
            self.assemble()
        self.assertFalse(self.output.exists())

    def test_preserves_previous_download_identity_and_mac_update_feed(self):
        previous = self.root / "previous"
        previous.mkdir()
        feed = previous / "appcast.xml"
        feed.write_bytes(b"https://github.com/sudosylabs/vnidrop/releases/download/v0.3.4/VniDrop-0.3.4.dmg")
        old = {"productVersion": "0.3.4", "releaseChannel": "beta", "tag": "v0.3.4", "files": [
            {"name": "VniDrop-0.3.4.dmg", "sha256": "a" * 64, "bytes": 123},
            {"name": "appcast.xml", "sha256": release.digest(feed), "bytes": feed.stat().st_size},
        ]}
        path = previous / "release-manifest.json"
        path.write_text(json.dumps(old))
        manifest = self.assemble(previous=path)
        self.assertEqual(manifest["downloads"]["macos"], {"version": "0.3.4", "tag": "v0.3.4", "files": [old["files"][0]]})
        self.assertEqual((self.output / "appcast.xml").read_bytes(), feed.read_bytes())
        self.assertFalse((self.output / "VniDrop-0.3.4.dmg").exists())
        shutil.rmtree(self.output)
        feed.write_bytes(b"tampered")
        with self.assertRaisesRegex(ValueError, "checksum"):
            self.assemble(previous=path)

    def test_rejects_wrong_tag_empty_selection_and_missing_store_package(self):
        for overrides in ({"tag": "v0.3.6"}, {"platforms": ""}, {"platforms": "ios"}, {"stores": "windows"}):
            with self.subTest(overrides=overrides), self.assertRaises(ValueError):
                self.assemble(**overrides)


if __name__ == "__main__":
    unittest.main()
