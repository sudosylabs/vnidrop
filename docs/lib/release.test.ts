import assert from "node:assert/strict";
import test from "node:test";
import { assetDownloadUrl, loadLatestRelease } from "./release.ts";

test("asset URLs pin the file to its release tag, not /latest/", () => {
  const url = assetDownloadUrl("v0.3.0", "VniDrop-0.3.0.dmg");
  assert.equal(
    url,
    "https://github.com/sudosylabs/vnidrop/releases/download/v0.3.0/VniDrop-0.3.0.dmg",
  );
  assert.equal(url.includes("/latest/"), false);
});

test("asset URLs encode names that would otherwise 404 on GitHub", () => {
  const url = assetDownloadUrl("v0.3.0", "vnidrop_0.3.0-1_amd64.deb");
  assert.equal(
    url,
    "https://github.com/sudosylabs/vnidrop/releases/download/v0.3.0/vnidrop_0.3.0-1_amd64.deb",
  );
});

test("latest release exposes the unsigned Windows direct installer", async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () =>
    new Response(
      JSON.stringify({
        productVersion: "0.3.2",
        releaseChannel: "beta",
        tag: "v0.3.2",
        files: [
          {
            name: "VniDrop_0.3.2_x64.exe",
            sha256: "windows-sha256",
            bytes: 42_000_000,
          },
        ],
      }),
    );

  try {
    const release = await loadLatestRelease();
    assert.deepEqual(release.windowsExe, {
      name: "VniDrop_0.3.2_x64.exe",
      url: "https://github.com/sudosylabs/vnidrop/releases/download/v0.3.2/VniDrop_0.3.2_x64.exe",
      bytes: 42_000_000,
      sha256: "windows-sha256",
    });
  } finally {
    globalThis.fetch = originalFetch;
  }
});
