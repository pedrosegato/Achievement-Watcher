import { FALLBACK_LOCALE } from "../constants";
import en from "./locales/en.json";

export type Key = keyof typeof en;
type Dictionary = Partial<Record<Key, string>>;

const FILE_NAME = /([^/]+)\.json$/;

const modules = import.meta.glob<Dictionary>("./locales/*.json", {
  eager: true,
  import: "default",
});

export const DICTIONARIES: Record<string, Dictionary> = Object.fromEntries(
  Object.entries(modules).map(([path, dictionary]) => [
    path.match(FILE_NAME)?.[1] ?? FALLBACK_LOCALE,
    dictionary,
  ]),
);

export const FALLBACK_DICTIONARY: Record<Key, string> = en;
