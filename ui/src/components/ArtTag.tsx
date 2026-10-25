import type { ReactNode } from "react";
import { Icon, type IconName } from "./Icon";

interface ArtTagProps {
  icon?: IconName;
  children: ReactNode;
}

export function ArtTag({ icon, children }: ArtTagProps) {
  return (
    <span className="flex items-center gap-1 rounded-tag bg-[rgba(8,9,11,0.72)] px-[7px] py-[3px] font-semibold text-[10.5px] text-white backdrop-blur-[4px]">
      {icon && <Icon name={icon} size={12} />}
      {children}
    </span>
  );
}
