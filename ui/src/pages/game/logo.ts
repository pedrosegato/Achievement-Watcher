import type { CSSProperties } from "react";
import type { LogoPin, LogoPosition } from "../../types/domain";

type HeroMode = "hero" | "header-blur" | "plain";

const FALLBACK: LogoPosition = { pin: "bottom-left", widthPct: 40, heightPct: 50 };

const ANCHORS: Record<LogoPin, CSSProperties> = {
  "bottom-left": { left: "3.5%", bottom: "11%" },
  "bottom-center": { left: "50%", bottom: "11%", translate: "-50% 0" },
  "center-center": { left: "50%", top: "55%", translate: "-50% -50%" },
  "upper-center": { left: "50%", top: "22%", translate: "-50% 0" },
  "upper-left": { left: "3.5%", top: "22%" },
};

const OBJECT_POSITIONS: Record<LogoPin, string> = {
  "bottom-left": "left bottom",
  "bottom-center": "center bottom",
  "center-center": "center",
  "upper-center": "center top",
  "upper-left": "left top",
};

export function logoBox(position: LogoPosition | null): CSSProperties {
  const { pin, widthPct, heightPct } = position ?? FALLBACK;
  return { "--w": widthPct, "--h": heightPct, ...ANCHORS[pin] } as CSSProperties;
}

export function logoObjectPosition(position: LogoPosition | null): string {
  return OBJECT_POSITIONS[(position ?? FALLBACK).pin];
}

export function heroFallback(hasHero: boolean, hasHeader: boolean): HeroMode {
  if (hasHero) return "hero";
  return hasHeader ? "header-blur" : "plain";
}
