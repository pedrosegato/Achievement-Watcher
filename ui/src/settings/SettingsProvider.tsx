import { type ReactNode, useEffect, useState } from "react";
import { EVENTS } from "../constants";
import { useTauriEvent } from "../hooks/useTauriEvent";
import { api } from "../lib/commands";
import type { Settings } from "../types/domain";
import { AppSettingsContext } from "./context";

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    let current = true;
    api
      .getSettings()
      .then((value) => {
        if (current) setSettings(value);
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, []);

  useTauriEvent<Settings>(EVENTS.settingsChanged, setSettings);

  return <AppSettingsContext value={settings}>{children}</AppSettingsContext>;
}
