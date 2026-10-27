import { useEffect, useState } from "react";
import { type PlayMoment, type PlayPhase, playMoment } from "../lib/play";

const MOMENT_MS: Record<PlayMoment, number> = {
  rumble: 420,
  synced: 1800,
};

export function usePlayMoment(phase: PlayPhase | undefined): PlayMoment | null {
  const [previous, setPrevious] = useState(phase);
  const [moment, setMoment] = useState<PlayMoment | null>(null);
  if (phase !== previous) {
    setPrevious(phase);
    setMoment(phase === undefined ? null : playMoment(previous, phase));
  }
  useEffect(() => {
    if (!moment) return;
    const timer = window.setTimeout(() => setMoment(null), MOMENT_MS[moment]);
    return () => window.clearTimeout(timer);
  }, [moment]);
  return moment;
}
