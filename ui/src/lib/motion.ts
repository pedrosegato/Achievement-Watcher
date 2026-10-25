import { useState } from "react";

const MS_PER_SECOND = 1000;

export function parseDuration(raw: string): number {
  const text = raw.trim();
  const value = Number.parseFloat(text);
  if (Number.isNaN(value)) return 0;
  if (text.endsWith("ms")) return value;
  return text.endsWith("s") ? value * MS_PER_SECOND : 0;
}

const tokens = new Map<string, number>();

function readToken(token: string): string {
  if (typeof document === "undefined") return "";
  return getComputedStyle(document.documentElement).getPropertyValue(token);
}

export function motionMs(token: string, read: (token: string) => string = readToken): number {
  let ms = tokens.get(token);
  if (ms === undefined) {
    ms = parseDuration(read(token));
    tokens.set(token, ms);
  }
  return ms;
}

export function useMotionMs(token: string): number {
  const [ms] = useState(() => motionMs(token));
  return ms;
}
