import { createContext } from "react";
import { FALLBACK_LOCALE } from "../constants";
import type { I18nValue } from "../types/ui";
import { translate } from "./translate";

export const I18nContext = createContext<I18nValue>({
  locale: FALLBACK_LOCALE,
  t: (key, vars) => translate(FALLBACK_LOCALE, key, vars),
  refresh: () => undefined,
});
