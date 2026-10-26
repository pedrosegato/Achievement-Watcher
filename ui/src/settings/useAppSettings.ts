import { use } from "react";
import { RARE_BADGE_PERCENT } from "../constants";
import type { Settings } from "../types/domain";
import { AppSettingsContext } from "./context";

export function useAppSettings(): Settings | null {
  return use(AppSettingsContext);
}

export function useRareThreshold(): number {
  return useAppSettings()?.rareThreshold ?? RARE_BADGE_PERCENT;
}
