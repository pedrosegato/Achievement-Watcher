import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { EVENTS } from "../constants";
import type { CloudActivity, CloudGameView } from "../types/domain";
import { useTauriEvent } from "./useTauriEvent";

const SHOW_AFTER_MS = 300;

export function useCloudActivity(appId: number): boolean {
  const client = useQueryClient();
  const [syncing, setSyncing] = useState(false);
  const timer = useRef<number | null>(null);
  useTauriEvent<CloudActivity>(EVENTS.cloudActivity, (activity) => {
    if (activity.appId !== appId) return;
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
    if (activity.syncing) {
      timer.current = window.setTimeout(() => setSyncing(true), SHOW_AFTER_MS);
    } else {
      setSyncing(false);
      const cached = client.getQueryData<CloudGameView>(["cloud-game", appId]);
      if (cached?.status.type === "syncing") {
        void client.invalidateQueries(
          { queryKey: ["cloud-game", appId] },
          { cancelRefetch: false },
        );
      }
    }
  });
  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );
  return syncing;
}
