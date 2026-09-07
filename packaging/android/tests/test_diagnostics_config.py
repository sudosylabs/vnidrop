"""Exercise the actual Gradle config generator using synthetic ingest credentials."""

import os
from pathlib import Path
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[3]
CONFIG = ROOT / "shared/build/generated/diagnostics/commonMain/kotlin/com/vnidrop/app/diagnostics/DiagnosticsBuildConfig.kt"


class DiagnosticsConfigTest(unittest.TestCase):
    def generate(self, *, endpoint="", key="", included=None, environment=None, succeeds=True):
        env = os.environ.copy()
        env.pop("VNIDROP_DIAGNOSTICS_ENDPOINT", None)
        env.pop("VNIDROP_DIAGNOSTICS_INGEST_KEY", None)
        env.update(environment or {})
        command = [
            str(ROOT / "gradlew"), ":shared:generateDiagnosticsBuildConfig",
            "--no-daemon", "--no-configuration-cache", "--console=plain", "--max-workers=2",
            f"-Pvnidrop.diagnostics.endpoint={endpoint}",
            f"-Pvnidrop.diagnostics.ingestKey={key}",
        ]
        if included is not None:
            command.append(f"-Pvnidrop.diagnostics.included={str(included).lower()}")
        result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode == 0, succeeds, "Unexpected Gradle configuration result")
        if not succeeds:
            self.assertIn("Bug reporting requires both", result.stdout + result.stderr)
            return ""
        return CONFIG.read_text()

    def test_user_properties_enable_delivery_without_an_extra_flag(self):
        config = self.generate(endpoint="https://reports.example.test", key="fixture-ingest-key")
        self.assertIn("INCLUDED: Boolean = true", config)
        self.assertIn('ENDPOINT: String = "https://reports.example.test"', config)
        self.assertIn('INGEST_KEY: String = "fixture-ingest-key"', config)

    def test_release_environment_overrides_properties(self):
        config = self.generate(endpoint="https://ignored.example.test", key="ignored", included=True, environment={
            "VNIDROP_DIAGNOSTICS_ENDPOINT": "https://ci.example.test",
            "VNIDROP_DIAGNOSTICS_INGEST_KEY": "ci-fixture-key",
        })
        self.assertIn('ENDPOINT: String = "https://ci.example.test"', config)
        self.assertIn('INGEST_KEY: String = "ci-fixture-key"', config)
        self.assertNotIn("ignored", config)

    def test_explicit_offline_build_omits_credentials(self):
        config = self.generate(endpoint="https://reports.example.test", key="fixture-ingest-key", included=False)
        self.assertIn("INCLUDED: Boolean = false", config)
        self.assertIn('ENDPOINT: String = ""', config)
        self.assertIn('INGEST_KEY: String = ""', config)

    def test_unconfigured_build_is_offline_but_release_fails(self):
        config = self.generate()
        self.assertIn("INCLUDED: Boolean = false", config)
        self.generate(included=True, succeeds=False)

    def test_partial_configuration_fails(self):
        self.generate(endpoint="https://reports.example.test", succeeds=False)
        self.generate(key="fixture-ingest-key", succeeds=False)


if __name__ == "__main__":
    unittest.main()
