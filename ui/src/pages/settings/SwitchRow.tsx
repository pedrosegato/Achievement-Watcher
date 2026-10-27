import type { ReactNode } from "react";
import { Switch } from "../../components/ui/switch";
import { SettingRow } from "./SettingRow";

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
