import { expect, test } from "bun:test";
import { join } from "node:path";
import { REPO_ROOT, STRINGS_JSON } from "../config";
import type { StringsFile } from "../types";
import {
  appleInfoPlistStrings,
  syncApplePrivacyFallbacks,
  syncLinuxDesktopComments,
  syncLinuxMimeComments,
} from "./system-metadata";

const doc = (await Bun.file(STRINGS_JSON).json()) as StringsFile;

test("Apple uses native permission keys with localized values and preserves fallbacks", () => {
  const french = appleInfoPlistStrings(doc, "fr");
  expect(french).toContain('"NSCameraUsageDescription" = "VniDrop utilise la caméra');
  expect(french).toContain('"NSLocalNetworkUsageDescription" = "VniDrop a besoin');
  const plist = "<key>NSCameraUsageDescription</key><string>Old</string>\n"
    + "<key>NSLocalNetworkUsageDescription</key><string>Old</string>";
  expect(syncApplePrivacyFallbacks(doc, plist)).toContain(
    "<key>NSCameraUsageDescription</key><string>VniDrop uses the camera",
  );
});

test("packaging comments are translated and regenerated without duplicates", async () => {
  const packaging = join(REPO_ROOT, "linux/packaging");
  const desktop = await Bun.file(join(packaging, "com.vnidrop.VniDrop.desktop")).text();
  const mime = await Bun.file(join(packaging, "com.vnidrop.VniDrop.xml")).text();
  expect(desktop).toContain("Comment[fr]=Envoyez des fichiers");
  expect(mime).toContain('<comment xml:lang="fr">Invitation de transfert VniDrop</comment>');
  expect(syncLinuxDesktopComments(doc, desktop)).toBe(desktop);
  expect(syncLinuxMimeComments(doc, mime)).toBe(mime);
});
