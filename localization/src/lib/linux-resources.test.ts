import { expect, test } from "bun:test";
import { renderLinuxResources } from "./linux-resources";
import { REPO_ROOT, STRINGS_JSON } from "../config";

test("committed Linux catalog matches the localization source", async () => {
  const source = await Bun.file(STRINGS_JSON).json();
  const committed = await Bun.file(`${REPO_ROOT}/linux/data/strings.json`).text();
  expect(committed).toBe(renderLinuxResources(source));
});

test("Linux catalogs preserve named arguments, plural categories and source fallbacks", () => {
  const result = JSON.parse(renderLinuxResources({sourceLanguage: "en", supportedLanguages: ["en", "fr"], strings: {
    title: {context: "Title", translations: {en: 'Files & "{name}"'}},
    apple: {context: "Apple", targets: ["apple"], translations: {en: "Apple only"}},
    files: {context: "Count", plural: {en: {one: "{count} file", other: "{count} files"}}},
  }}));
  expect(result.languages.fr).toEqual({title: 'Files & "{name}"', files: {one: "{count} file", other: "{count} files"}});
});

test("literal Linux UI resource references are present in the generated catalog", async () => {
  const catalog = JSON.parse(renderLinuxResources(await Bun.file(STRINGS_JSON).json()));
  let references = 0;
  for await (const path of new Bun.Glob("src/**/*.rs").scan(`${REPO_ROOT}/linux`)) {
    if (path.endsWith("preferences.rs")) continue;
    const source = await Bun.file(`${REPO_ROOT}/linux/${path}`).text();
    for (const match of source.matchAll(/"((?:linux|button|error|progress|status|transfer|send|receive|preferences|field|approval|app)_[a-z_]+)"/g)) {
      expect(catalog.languages.en[match[1]!], `${path}: ${match[1]}`).toBeDefined();
      references++;
    }
  }
  expect(references).toBeGreaterThan(0);
});
