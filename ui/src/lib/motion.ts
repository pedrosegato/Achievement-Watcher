import { useState } from "react";

const MS_PER_SECOND = 1000;

export function parseDuration(raw: string): number {
  const text = raw.trim();
  const value = Number.parseFloat(text);
  if (Number.isNaN(value)) return 0;
  if (text.endsWith("ms")) return value;
  return text.endsWith("s") ? value * MS_PER_SECOND : 0;
}

function readMotionMs(token: string): number {
  if (typeof document === "undefined") return 0;
  return parseDuration(getComputedStyle(document.documentElement).getPropertyValue(token));
}

export function useMotionMs(token: string): number {
  const [ms] = useState(() => readMotionMs(token));
  return ms;
}
