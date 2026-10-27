import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { EVENTS } from "../constants";
import { useI18n } from "../i18n/useI18n";
import { cloudErrorKey } from "../lib/cloud";
import { errorMessage } from "../lib/commands";
import { cloudChangedKeys } from "../lib/events";
import type { CloudActivity } from "../types/domain";
import { useTauriEvent } from "./useTauriEvent";

const QUEUED_TIMEOUT_MS = 20_000;

export type Run = (work: () => Promise<void>, options?: { queued?: boolean }) => Promise<void>;

export function useCloudAction(appId?: number) {
  const { t } = useI18n();
  const client = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const timer = useRef<number | null>(null);

  const settle = useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
    setBusy(false);
  }, []);
  useTauriEvent<CloudActivity>(EVENTS.cloudActivity, (activity) => {
    if (activity.appId === appId && !activity.syncing) settle();
  });
  useTauriEvent<number | null>(EVENTS.cloudChanged, (id) => {
    if (id === appId || id === null) settle();
  });
  useEffect(() => settle, [settle]);

  const attempt = useCallback(
    async (work: () => Promise<void>, refresh: boolean): Promise<boolean> => {
      try {
        await work();
        if (refresh) {
          await Promise.all(
            cloudChangedKeys(appId ?? null).map((queryKey) =>
              client.invalidateQueries({ queryKey }, { cancelRefetch: false }),
            ),
          );
        }
        return true;
      } catch (cause) {
        const code = errorMessage(cause);
        const key = cloudErrorKey(code);
        setError(key ? t(key) : code);
        return false;
      }
    },
    [appId, client, t],
  );

  const run: Run = useCallback(
    async (work, options) => {
      setError(null);
      const queued = options?.queued === true;
      if (queued) {
        setBusy(true);
        timer.current = window.setTimeout(settle, QUEUED_TIMEOUT_MS);
      }
      const done = await attempt(work, !queued);
      if (!done) settle();
    },
    [attempt, settle],
  );
  const clearError = useCallback(() => setError(null), []);

  return { error, busy, run, clearError };
}
