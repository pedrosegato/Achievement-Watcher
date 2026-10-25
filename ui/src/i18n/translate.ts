import type { Locale } from "../types/domain";
import type { Vars } from "../types/ui";
import { DICTIONARIES, FALLBACK_DICTIONARY, type Key } from "./dictionary";

const PLACEHOLDER = /\{(\w+)\}/g;

export function translate(locale: Locale, key: Key, vars?: Vars): string {
  const template = DICTIONARIES[locale]?.[key] ?? FALLBACK_DICTIONARY[key];
  if (!vars) return template;
  return template.replace(PLACEHOLDER, (match, name: string) =>
    name in vars ? String(vars[name]) : match,
  );
}
