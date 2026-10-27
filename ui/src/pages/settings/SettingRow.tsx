import type { ReactNode } from "react";

interface SettingRowProps {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
}

export function SettingRow({ label, hint, children }: SettingRowProps) {
  return (
    <div className="flex items-center justify-between gap-5 border-line border-b py-3.5">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="font-semibold text-[14px]">{label}</span>
        {hint && <span className="text-[12.5px] text-dim">{hint}</span>}
      </div>
      <div className="flex flex-none items-center gap-2">{children}</div>
    </div>
  );
}
