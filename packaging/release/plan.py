#!/usr/bin/env python3
"""Resolve requested platforms into build and publication destinations."""

import json
import os
from pathlib import Path
import re
import sys


PLATFORMS = ("windows", "linux", "android", "macos", "ios")


def previous_release(pages, version):
    releases = []
    for page in pages:
        if not isinstance(page, list):
            raise ValueError("Invalid GitHub release history")
        for release in page:
            tag = release.get("tag_name", "")
            if release.get("draft") is False and release.get("prerelease") is False and re.fullmatch(r"v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", tag):
                releases.append((tuple(map(int, tag[1:].split("."))), tag))
    if not releases:
        return ""
    previous_version, tag = max(releases)
    if tuple(map(int, version.split("."))) <= previous_version:
        raise ValueError(f"Release version must be newer than {tag}")
    return tag


def plan(inputs, preview=False):
    distribution = "direct" if preview else inputs.get("distribution", "both")
    distribution = {"Direct downloads + stores": "both", "Direct downloads": "direct", "Stores": "stores"}.get(distribution, distribution)
    if distribution not in ("direct", "stores", "both"):
        raise ValueError("Distribution must be direct, stores, or both")
    selected = []
    for platform in PLATFORMS:
        value = inputs.get(platform, False)
        if not isinstance(value, bool):
            raise ValueError(f"{platform} must be a boolean")
        if value:
            selected.append(platform)
    direct = [p for p in selected if p != "ios" and distribution != "stores"]
    stores = [p for p in selected if p != "linux" and distribution != "direct"]
    if not direct and not stores:
        raise ValueError("Select at least one platform supported by the distribution")
    apple = [p for p in ("ios", "macos") if p in stores]
    return {
        "direct_platforms": ",".join(direct),
        "store_platforms": ",".join(stores),
        "windows": "windows" in direct or "windows" in stores,
        "linux": "linux" in direct,
        "android": "android" in direct or "android" in stores,
        "macos": "macos" in direct,
        "windows_store": "windows" in stores,
        "android_store": "android" in stores,
        "apple_store": bool(apple),
        "apple_platforms": "both" if len(apple) == 2 else next(iter(apple), ""),
    }


if __name__ == "__main__":
    try:
        if sys.argv[1:] == ["--previous"]:
            pages = json.loads(Path(os.environ["RELEASE_HISTORY"]).read_text(encoding="utf-8"))
            tag = previous_release(pages, os.environ["VERSION"])
            with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
                output.write(f"previous_tag={tag}\n")
            raise SystemExit(0)
        result = plan(json.loads(os.environ["RELEASE_INPUTS"]), os.environ.get("PREVIEW") == "true")
        with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
            for key, value in result.items():
                output.write(f"{key}={str(value).lower() if isinstance(value, bool) else value}\n")
        with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a", encoding="utf-8") as summary:
            summary.write(f"Direct downloads: {result['direct_platforms'] or 'none'}\n\n")
            summary.write(f"Store submissions: {result['store_platforms'] or 'none'}\n")
    except (ValueError, KeyError) as error:
        raise SystemExit(str(error)) from error
