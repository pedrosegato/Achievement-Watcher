import type { ReactNode } from "react";
import { Switch } from "../../components/ui/switch";

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

export function SettingBlock({ label, hint, children }: SettingRowProps) {
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

interface SwitchRowProps {
  label: string;
  hint?: ReactNode;
  checked: boolean;
  onChange: (checked: boolean) => void;
}

export function SwitchRow({ label, hint, checked, onChange }: SwitchRowProps) {
  return (
    <SettingRow label={label} hint={hint}>
      <Switch aria-label={label} checked={checked} onCheckedChange={onChange} />
    </SettingRow>
  );
}
