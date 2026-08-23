import assert from "node:assert/strict";
import test from "node:test";
import { assetDownloadUrl } from "./release.ts";

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
