import type { ReactNode } from "react";

interface SettingBlockProps {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
}

export function SettingBlock({ label, hint, children }: SettingBlockProps) {
  return (
    <div className="flex flex-col gap-2.5 border-line border-b py-3.5">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="font-semibold text-[14px]">{label}</span>
        {hint && <span className="text-[12.5px] text-dim">{hint}</span>}
      </div>
      {children}
    </div>
  );
}
