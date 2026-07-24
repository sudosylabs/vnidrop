import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Repo root = parent of the localization/ directory. */
export const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const LOC_DIR = join(REPO_ROOT, "localization");
export const STRINGS_JSON = join(LOC_DIR, "strings.json");

/** Apple String Catalog — one file, all languages nested. */
export const APPLE_XCSTRINGS = join(
  REPO_ROOT,
  "apple/VniDrop/Resources/Localizable.xcstrings",
);

/** Apple app Info.plist — its CFBundleLocalizations is kept in sync with supportedLanguages. */
export const APPLE_INFO_PLIST = join(
  REPO_ROOT,
  "apple/VniDrop/Resources/Info.plist",
);

/** Generated Swift accessors (`L10n`) for compile-time-checked catalog keys. */
export const APPLE_L10N_SWIFT = join(
  REPO_ROOT,
  "apple/VniDrop/Generated/L10n.swift",
);

/** KMP / Compose Multiplatform resources root; one values[-lang]/strings.xml per language. */
export const KMP_RESOURCES = join(
  REPO_ROOT,
  "shared/src/commonMain/composeResources",
);

/** Directory holding a language's strings.xml under the compose resources root. */
export function kmpValuesDir(lang: string, sourceLanguage: string): string {
  return lang === sourceLanguage ? "values" : `values-${lang}`;
}
