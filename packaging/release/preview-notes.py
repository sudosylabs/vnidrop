"""Render download notes from an assembled preview manifest."""

import json
import sys
from pathlib import Path


def render(manifest):
    guidance = {
        "macos": "The macOS DMG is Developer ID signed and notarized. It uses the existing app identity and stable update feed.",
        "windows": "The x64 Windows installer uses the existing direct-install app identity. It is unsigned and SmartScreen may warn.",
        "linux": "The x64 DEB (Ubuntu 24.04+) and RPM (Fedora 43) replace the installed VniDrop package.",
        "android": "The preview APK supports arm64-v8a and x86_64. Its dedicated signing key and package name let it coexist with the Play Store app, with separate data.",
    }
    lines = ["Preview for early testing, built in Release configuration with bug reporting enabled.", ""]
    lines.extend(f"- `{item['name']}`" for item in manifest["files"])
    lines.append("")
    lines.extend(guidance[platform] for platform in manifest["platforms"])
    if any(platform != "android" for platform in manifest["platforms"]):
        lines.extend(["", "Desktop packages use the base product version. Close VniDrop before installing. "
                      "Switching previews of the same version may require reinstalling the package."])
    lines.extend(["", "The preview number appears in the filenames and preview-manifest.json. "
                  "SHA256SUMS covers every download. Install preview updates manually."])
    return "\n".join(lines)


if __name__ == "__main__":
    print(render(json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))))
