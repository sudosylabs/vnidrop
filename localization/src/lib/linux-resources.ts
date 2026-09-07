import { targetsOf, type StringsFile } from "../types";

export function renderLinuxResources(doc: StringsFile): string {
  const languages: Record<string, Record<string, unknown>> = {};
  for (const language of doc.supportedLanguages) {
    const strings: Record<string, unknown> = {};
    for (const [key, entry] of Object.entries(doc.strings).sort(([a], [b]) => a.localeCompare(b))) {
      if (!targetsOf(entry).includes("linux")) continue;
      strings[key] = entry.plural
        ? entry.plural[language] ?? entry.plural[doc.sourceLanguage]
        : entry.translations?.[language] ?? entry.translations?.[doc.sourceLanguage] ?? key;
    }
    languages[language] = strings;
  }
  return JSON.stringify({ sourceLanguage: doc.sourceLanguage, languages }) + "\n";
}
