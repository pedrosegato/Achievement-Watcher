import type { CSSProperties } from "react";
import { cn } from "../lib/cn";
import { AppLogo } from "./AppLogo";

type AchievementIconSize = 20 | 22 | 26 | 28 | 30 | 34 | 40 | 46 | 56;

interface AchievementIconProps {
  src?: string | null;
  size: AchievementIconSize;
  gray?: boolean;
  rare?: boolean;
  platinum?: boolean;
  className?: string;
}

export function AchievementIcon({
  src,
  size,
  gray = false,
  rare = false,
  platinum = false,
  className,
}: AchievementIconProps) {
  const glow = rare || platinum;
  const style = { "--icon": `${size}px` } as CSSProperties;
  let face = <span className="relative z-1 block size-full bg-surface-2" />;
  if (platinum) {
    face = (
      <span className="plat-tile">
        <AppLogo className="size-[68%]" />
      </span>
    );
  } else if (src) {
    face = (
      <img
        src={src}
        alt=""
        loading="lazy"
        draggable={false}
        className={cn("relative z-1 block size-full", gray && "grayscale")}
      />
    );
  }
  return (
    <span
      className={cn(
        "relative grid size-(--icon) flex-none place-items-center",
        glow && "rare-icon",
        className,
      )}
      style={style}
    >
      {glow && <i className="rare-rim" aria-hidden="true" />}
      {face}
    </span>
  );
}
