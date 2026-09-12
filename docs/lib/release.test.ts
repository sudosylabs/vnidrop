import assert from "node:assert/strict";
import test from "node:test";
import { assetDownloadUrl, loadLatestRelease, loadLatestPreview, releaseFromManifest, selectLatestPreview } from "./release.ts";

test("asset URLs pin the file to its release tag, not /latest/", () => {
  const url = assetDownloadUrl("v0.3.0", "VniDrop-0.3.0.dmg");
  assert.equal(
    url,
    "https://github.com/sudosylabs/vnidrop/releases/download/v0.3.0/VniDrop-0.3.0.dmg",
  );
  assert.equal(url.includes("/latest/"), false);
});

function preview(number: number, publishedAt = "2026-09-08T12:00:00Z") {
  const label = `0.3.3-preview.${number}`;
  return {
    tag_name: `preview-0.3.3-${number}`,
    draft: false,
    prerelease: true,
    published_at: publishedAt,
    assets: [
      `VniDrop-${label}-arm64.dmg`, `VniDrop-${label}-x64.exe`,
      `vnidrop_${label}_amd64.deb`, `vnidrop-${label}.x86_64.rpm`,
      `VniDrop-${label}.apk`, "SHA256SUMS", "preview-manifest.json",
    ].map((name) => ({
      name, state: "uploaded", size: 1024, digest: `sha256:${"a".repeat(64)}`,
      browser_download_url: "https://untrusted.example/download",
    })),
  };
}

test("preview downloads pin all five packages and metadata to the validated tag", () => {
  const release = selectLatestPreview([preview(17)]);
  assert.ok(release);
  assert.equal(release.tag, "preview-0.3.3-17");
  assert.equal(release.version, "0.3.3");
  assert.equal(release.number, 17);
  for (const asset of [release.dmg, release.windowsExe, release.deb, release.rpm, release.apk]) {
    assert.ok(asset);
    assert.equal(asset.url, assetDownloadUrl(release.tag, asset.name));
    assert.equal(asset.sha256, "a".repeat(64));
    assert.equal(asset.bytes, 1024);
  }
  assert.equal(release.manifestUrl, assetDownloadUrl(release.tag, "preview-manifest.json"));
  assert.equal(release.checksumsUrl, assetDownloadUrl(release.tag, "SHA256SUMS"));
});

test("preview selection skips drafts, other channels, and incomplete or malformed releases", () => {
  const valid = preview(1);
  const candidates = [
    { ...preview(2), draft: true },
    { ...preview(3), prerelease: false },
    { ...preview(4), tag_name: "v0.3.3-beta.4" },
    { ...preview(5), tag_name: "preview-0.3.3-01" },
    { ...preview(6), published_at: "invalid date" },
    { ...preview(7), assets: preview(7).assets.filter((asset) => !asset.name.endsWith(".rpm")) },
    { ...preview(8), assets: preview(8).assets.slice(0, -1) },
    { ...preview(9), assets: preview(9).assets.filter((asset) => asset.name !== "SHA256SUMS") },
    { ...preview(10), assets: preview(10).assets.map((asset) => ({ ...asset, state: "starter" })) },
    { ...preview(11), assets: preview(11).assets.map((asset) => ({ ...asset, size: 0 })) },
    { ...preview(12), assets: [...preview(12).assets, preview(12).assets[0]] },
    null,
  ];
  for (const candidate of candidates) {
    assert.equal(selectLatestPreview([candidate]), null);
    assert.equal(selectLatestPreview([candidate, valid])?.number, 1);
  }
});

test("preview selection uses publication date rather than response order or version", () => {
  const earlier = preview(20, "2026-09-07T12:00:00Z");
  const later = preview(19, "2026-09-08T12:00:00Z");
  assert.equal(selectLatestPreview([earlier, later])?.number, 19);
  assert.equal(selectLatestPreview([later, earlier])?.number, 19);
  assert.equal(selectLatestPreview([]), null);
  assert.throws(() => selectLatestPreview({ message: "API rate limit exceeded" }), /Invalid/);
});

test("missing GitHub digest does not hide a complete preview with a checksum file", () => {
  const release = preview(17);
  release.assets = release.assets.map((asset) => ({ ...asset, digest: "" }));
  assert.equal(selectLatestPreview([release])?.apk?.sha256, undefined);
  assert.ok(selectLatestPreview([release])?.checksumsUrl);
});

test("preview API distinguishes an empty list from a failed request and passes cancellation", async (t) => {
  const controller = new AbortController();
  const fetchMock = t.mock.method(globalThis, "fetch", async (url: string, init: RequestInit) => {
    assert.equal(url, "https://api.github.com/repos/sudosylabs/vnidrop/releases?per_page=100");
    assert.equal(init.signal, controller.signal);
    return new Response("[]");
  });
  assert.equal(await loadLatestPreview(controller.signal), null);
  fetchMock.mock.mockImplementation(async () => new Response("rate limited", { status: 403 }));
  await assert.rejects(loadLatestPreview(), /403/);
});

test("a preview manifest cannot replace the normal release downloads", async (t) => {
  t.mock.method(globalThis, "fetch", async () => new Response(JSON.stringify({
    productVersion: "0.3.3", releaseChannel: "preview", tag: "preview-0.3.3-17", files: [],
  })));
  await assert.rejects(loadLatestRelease(), /normal version tag/);
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
      version: "0.3.2",
      tag: "v0.3.2",
      checksumsUrl: assetDownloadUrl("v0.3.2", "SHA256SUMS"),
      name: "VniDrop_0.3.2_x64.exe",
      url: "https://github.com/sudosylabs/vnidrop/releases/download/v0.3.2/VniDrop_0.3.2_x64.exe",
      bytes: 42_000_000,
      sha256: "windows-sha256",
    });
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("partial releases preserve each platform's original version, URL, and checksums", () => {
  const windows = { name: "VniDrop_0.3.5_x64.exe", sha256: "a".repeat(64), bytes: 42 };
  const macos = { name: "VniDrop-0.3.4.dmg", sha256: "b".repeat(64), bytes: 43 };
  const release = releaseFromManifest({
    productVersion: "0.3.5", releaseChannel: "beta", tag: "v0.3.5", files: [windows],
    downloads: {
      windows: { version: "0.3.5", tag: "v0.3.5", files: [windows] },
      macos: { version: "0.3.4", tag: "v0.3.4", files: [macos] },
    },
  });
  assert.deepEqual(release.dmg, {
    ...macos, version: "0.3.4", tag: "v0.3.4", url: assetDownloadUrl("v0.3.4", macos.name),
    checksumsUrl: assetDownloadUrl("v0.3.4", "SHA256SUMS"),
  });
  assert.equal(release.windowsExe?.tag, "v0.3.5");
  assert.equal(release.apk, undefined);
});

test("download index rejects wrong tags, future versions, and path traversal", () => {
  for (const entry of [
    { version: "0.3.4", tag: "preview-0.3.4-1", files: [] },
    { version: "0.3.6", tag: "v0.3.6", files: [] },
    { version: "0.3.4", tag: "v0.3.4", files: [{ name: "../VniDrop-0.3.4.dmg", sha256: "a".repeat(64), bytes: 10 }] },
  ]) {
    assert.throws(() => releaseFromManifest({
      productVersion: "0.3.5", releaseChannel: "beta", tag: "v0.3.5", files: [], downloads: { macos: entry },
    }));
  }
});

test("a Windows-only preview retains older Apple, Android, and Linux downloads", () => {
  const old = preview(17, "2026-09-07T12:00:00Z");
  const current = preview(18);
  current.assets = current.assets.filter((file) => /\.exe$|\.json$|SHA256SUMS/.test(file.name));
  const selected = selectLatestPreview([old, current]);
  assert.equal(selected?.windowsExe?.tag, current.tag_name);
  for (const asset of [selected?.dmg, selected?.apk, selected?.deb, selected?.rpm]) {
    assert.ok(asset);
    assert.equal(asset.tag, old.tag_name);
    assert.equal(asset.checksumsUrl, assetDownloadUrl(old.tag_name, "SHA256SUMS"));
  }
  assert.equal(selectLatestPreview([current])?.dmg, undefined);
});
