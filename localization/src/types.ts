/** Canonical localization data model. See ../README.md for the full spec. */

/** Output targets a string can be emitted to. */
export type Target = "kmp" | "apple" | "windows";
export const ALL_TARGETS: Target[] = ["kmp", "apple", "windows"];

/** Argument types, used to pick the right platform placeholder token. */
export type ArgType = "string" | "int" | "double";

export interface Arg {
  /** Name referenced inside translations as `{name}`. */
  name: string;
  type: ArgType;
}

/** CLDR plural categories. `other` is always required when `plural` is used. */
export type PluralCategory = "zero" | "one" | "two" | "few" | "many" | "other";
export type PluralForms = Partial<Record<PluralCategory, string>> & {
  other: string;
};

export interface StringEntry {
  /** Where the string appears / its purpose. Emitted as xcstrings comment + xml comment. */
  context: string;
  /** Defaults to all targets when omitted. */
  targets?: Target[];
  /** Ordered positional args referenced as `{name}` in translations. */
  args?: Arg[];
  /** Flat (non-plural) translations, keyed by language. Mutually exclusive with `plural`. */
  translations?: Record<string, string>;
  /** Plural translations, keyed by language then CLDR category. Mutually exclusive with `translations`. */
  plural?: Record<string, PluralForms>;
}

export interface StringsFile {
  sourceLanguage: string;
  supportedLanguages: string[];
  strings: Record<string, StringEntry>;
}

export function targetsOf(entry: StringEntry): Target[] {
  return entry.targets ?? ALL_TARGETS;
}
