import type { ReactNode } from "react";
import { Icon, type IconName } from "./Icon";

interface FactProps {
  icon?: IconName;
  label: string;
  value: ReactNode;
  lead?: ReactNode;
}

export function Fact({ icon, label, value, lead }: FactProps) {
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      {icon && <Icon name={icon} size={22} className="flex-none text-dim" />}
      {lead}
      <div className="flex min-w-0 flex-col">
        <span className="text-[11.5px] text-dim">{label}</span>
        <span className="truncate font-semibold text-[14px]">{value}</span>
      </div>
    </div>
  );
}
