/** Structural checks on strings.json. Exits non-zero on errors (warnings don't fail). */
import { STRINGS_JSON } from "../config";
import type { StringsFile } from "../types";

const TOKEN = /\{([A-Za-z_][A-Za-z0-9_]*)\}/g;
const PLATFORM_PLACEHOLDER = /%(?:\d+\$)?(?:l{0,2})?[@sdif]/;

export async function validate() {
  const doc = JSON.parse(await Bun.file(STRINGS_JSON).text()) as StringsFile;
  const errors: string[] = [];
  const warnings: string[] = [];

  for (const [key, entry] of Object.entries(doc.strings)) {
    const where = `"${key}"`;
    const argNames = new Set((entry.args ?? []).map((a) => a.name));

    if (entry.translations && entry.plural) {
      errors.push(`${where}: has both "translations" and "plural" (pick one).`);
    }
    if (!entry.translations && !entry.plural) {
      warnings.push(`${where}: no translations yet.`);
    }
    if (!entry.context?.trim() || entry.context.startsWith("TODO")) {
      warnings.push(`${where}: missing/placeholder context.`);
    }

    const texts: string[] = [];
    if (entry.translations) texts.push(...Object.values(entry.translations));
    if (entry.plural) {
      for (const [lang, forms] of Object.entries(entry.plural)) {
        if (!forms.other?.trim()) errors.push(`${where} [${lang}]: plural missing required "other".`);
        texts.push(...Object.values(forms));
      }
    }

    for (const text of texts) {
      if (argNames.size > 0 && PLATFORM_PLACEHOLDER.test(text)) {
        errors.push(`${where}: uses a platform printf placeholder; use declared {name} placeholders in strings.json.`);
      }
      for (const m of text.matchAll(TOKEN)) {
        if (!argNames.has(m[1]!)) {
          errors.push(`${where}: uses {${m[1]}} but no matching arg is declared.`);
        }
      }
    }

    // Missing-language coverage (warn only).
    for (const lang of doc.supportedLanguages) {
      const present = entry.plural ? !!entry.plural[lang] : entry.translations?.[lang] !== undefined;
      const anyTranslations = entry.translations || entry.plural;
      if (anyTranslations && !present) warnings.push(`${where}: missing "${lang}".`);
    }
  }

  for (const w of warnings) console.warn(`warn  ${w}`);
  for (const e of errors) console.error(`error ${e}`);
  console.log(`\n${Object.keys(doc.strings).length} keys — ${errors.length} error(s), ${warnings.length} warning(s).`);
  if (errors.length) process.exit(1);
}
