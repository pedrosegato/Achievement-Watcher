import { useCallback, useEffect, useRef, useState } from "react";
import { EVENTS, TIMINGS } from "../constants";
import { useI18n } from "../i18n/useI18n";
import { api, errorMessage } from "../lib/commands";
import type { Settings } from "../types/domain";
import type { SettingsState, UpdateSetting } from "../types/ui";
import { useTauriEvent } from "./useTauriEvent";

export function useSettings(): SettingsState {
  const { refresh } = useI18n();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [revision, setRevision] = useState(0);
  const current = useRef<Settings | null>(null);

  const apply = useCallback((next: Settings) => {
    current.current = next;
    setSettings(next);
  }, []);

  useTauriEvent<Settings>(EVENTS.settingsChanged, apply);

  useEffect(() => {
    api
      .getSettings()
      .then(apply)
      .catch((reason: unknown) => setError(errorMessage(reason)));
  }, [apply]);

  useEffect(() => {
    if (!saved) return;
    const timer = window.setTimeout(() => setSaved(false), TIMINGS.statusFlashMs);
    return () => window.clearTimeout(timer);
  }, [saved]);

  const save = useCallback(
    async (next: Settings) => {
      apply(next);
      try {
        const stored = await api.saveSettings(next);
        apply(stored);
        refresh();
        setSaved(true);
        setRevision((value) => value + 1);
        setError(null);
      } catch (reason) {
        setError(errorMessage(reason));
      }
    },
    [apply, refresh],
  );

  const update = useCallback<UpdateSetting>(
    (key, value) => {
      return current.current ? save({ ...current.current, [key]: value }) : Promise.resolve();
    },
    [save],
  );

  return { settings, error, saved, revision, update };
}
