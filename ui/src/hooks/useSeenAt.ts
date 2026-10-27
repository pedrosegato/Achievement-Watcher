import { useState } from "react";

function read(key: string): number | null {
  try {
    const stored = Number(window.localStorage.getItem(key));
    return Number.isFinite(stored) && stored > 0 ? stored : null;
  } catch {
    return null;
  }
}

export function useSeenAt(key: string): [number | null, (at: number) => void] {
  const [seenAt, setSeenAt] = useState<number | null>(() => read(key));
  const markSeen = (at: number) => {
    setSeenAt(at);
    try {
      window.localStorage.setItem(key, String(at));
    } catch {
      return;
    }
  };
  return [seenAt, markSeen];
}
