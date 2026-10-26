import { useCallback, useEffect, useMemo, useState } from "react";
import { FALLBACK_LOCALE } from "../constants";
import { api } from "../lib/commands";
import type { Locale } from "../types/domain";
import type { ChildrenProps, Translate } from "../types/ui";
import { I18nContext } from "./context";
import { translate } from "./translate";

export function I18nProvider({ children }: ChildrenProps) {
  const [locale, setLocale] = useState<Locale>(FALLBACK_LOCALE);
  const [requests, setRequests] = useState(0);
  const refresh = useCallback(() => setRequests((count) => count + 1), []);

  useEffect(() => {
    if (requests < 0) return;
    let current = true;
    api
      .uiLocale()
      .then((code) => {
        if (current) setLocale(code);
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [requests]);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  const t = useCallback<Translate>((key, vars) => translate(locale, key, vars), [locale]);
  const value = useMemo(() => ({ locale, t, refresh }), [locale, t, refresh]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}
