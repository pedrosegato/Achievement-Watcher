import { useState } from "react";
import { Button } from "../../../../components/ui/button";
import { useCloudAction } from "../../../../hooks/useCloudAction";
import { useI18n } from "../../../../i18n/useI18n";
import { api } from "../../../../lib/commands";
import { SettingRow } from "../../SettingRow";

const INPUT =
  "w-[220px] rounded-card border border-line bg-surface px-2.5 py-1.5 text-[13px] outline-none focus-visible:border-text";

export function DeviceNameRow({ current }: { current: string }) {
  const { t } = useI18n();
  const { error, run } = useCloudAction();
  const [name, setName] = useState<string | null>(null);
  const changed = name !== null && name !== current;
  const save = () => {
    if (!changed) return;
    void run(() => api.setDeviceName(name));
  };

  return (
    <SettingRow label={t("cloud.deviceName")}>
      <form
        className="flex items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          save();
        }}
      >
        <input
          aria-label={t("cloud.deviceName")}
          value={name === null || name === current ? current : name}
          maxLength={40}
          onChange={(event) => setName(event.target.value)}
          className={INPUT}
        />
        {changed && (
          <Button size="sm" variant="primary" type="submit">
            {t("cloud.save")}
          </Button>
        )}
      </form>
      {error && (
        <span role="alert" className="text-[12.5px] text-text">
          {error}
        </span>
      )}
    </SettingRow>
  );
}
