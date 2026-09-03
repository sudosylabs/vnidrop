import { expect, test } from "bun:test";
import { renderWindowsResources } from "./windows-resources";
import { targetsOf } from "../types";
import { REPO_ROOT, STRINGS_JSON } from "../config";

test("Windows output escapes XML, preserves named arguments and falls back to the source language", () => {
  const output = renderWindowsResources({sourceLanguage: "en", supportedLanguages: ["en", "fr"], strings: {
    title: { context: "Title", translations: { en: 'Files & <folders> "{name}"' } },
    apple: { context: "Apple", targets: ["apple"], translations: { en: "Apple only" } },
    files: { context: "Count", plural: { en: { one: "{count} file", other: "{count} files" } } },
  }}, "fr");
  expect(output).toContain("Files &amp; &lt;folders&gt; &quot;{name}&quot;");
  expect(output).not.toContain("Apple only");
  expect(output).toContain('name="files_one"');
  expect(output).toContain("{count} files");
});

test("every literal Windows UI resource reference is emitted to Windows", async () => {
  const doc = await Bun.file(STRINGS_JSON).json();
  for await (const path of new Bun.Glob("{VniDrop,VniDrop.Core}/**/*.{cs,xaml}").scan(`${REPO_ROOT}/windows`)) {
    if (path.replaceAll("\\", "/").match(/\/(bin|obj)\//)) continue;
    const text = await Bun.file(`${REPO_ROOT}/windows/${path}`).text();
    const pattern = path.replaceAll("\\", "/").startsWith("VniDrop.Core/")
      ? /["']((?:windows|error)_[a-z_]+)["']/g
      : /["']((?:windows|app|button|field|send|receive|saved_devices|approval|error|status|storage|relay|theme|settings|nav|folder|notification)_[a-z_]+)["']/g;
    for (const match of text.matchAll(pattern)) {
      const entry = doc.strings[match[1]!];
      expect(entry, `${path}: ${match[1]}`).toBeDefined();
      expect(targetsOf(entry), `${path}: ${match[1]}`).toContain("windows");
    }
  }
});
