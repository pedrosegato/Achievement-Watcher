import { useSyncExternalStore } from "react";
import type { Theme } from "../types/domain";

type ResolvedTheme = Exclude<Theme, "system">;

const LIGHT_QUERY = "(prefers-color-scheme: light)";

export function resolveTheme(setting: Theme, prefersLight: boolean): ResolvedTheme {
  if (setting === "system") return prefersLight ? "light" : "dark";
  return setting;
}

function subscribe(onChange: () => void) {
  const media = window.matchMedia(LIGHT_QUERY);
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}

function prefersLight() {
  return window.matchMedia(LIGHT_QUERY).matches;
}

export function useResolvedTheme(setting: Theme): ResolvedTheme {
  const light = useSyncExternalStore(subscribe, prefersLight, () => false);
  return resolveTheme(setting, light);
}
