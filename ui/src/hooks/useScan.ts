import { useCallback, useState } from "react";
import { EVENTS, SCAN_DONE } from "../constants";
import { api } from "../lib/commands";
import { useTauriEvent } from "./useTauriEvent";

interface ScanState {
  scanning: number | null;
  start: () => void;
}

export function useScan(): ScanState {
  const [scanning, setScanning] = useState<number | null>(null);

  useTauriEvent<number>(EVENTS.scanProgress, (percent) =>
    setScanning(percent >= SCAN_DONE ? null : percent),
  );

  const start = useCallback(() => {
    setScanning((current) => current ?? 0);
    api
      .rescan()
      .then((started) => {
        if (!started) setScanning(null);
      })
      .catch(() => setScanning(null));
  }, []);

  return { scanning, start };
}
