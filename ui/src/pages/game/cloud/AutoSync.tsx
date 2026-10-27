import { useId } from "react";
import { Switch } from "../../../components/ui/switch";
import { useI18n } from "../../../i18n/useI18n";
import { api } from "../../../lib/commands";
import type { CloudPanelProps } from "./types";

export function AutoSync({ appId, view, busy, run }: CloudPanelProps) {
  const { t } = useI18n();
  const id = useId();
  if (!view.key) return null;

  return (
    <div className="flex items-center justify-between gap-3 border-line border-t px-6 py-3.5">
      <label htmlFor={id} className="cursor-pointer font-medium text-[13px]">
        {t("cloud.panel.autoSync")}
      </label>
      <Switch
        id={id}
        checked={view.enabled}
        disabled={busy}
        onCheckedChange={(enabled) => void run(() => api.setGameSync(appId, enabled))}
      />
    </div>
  );
}
