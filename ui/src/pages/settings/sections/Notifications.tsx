import { useQuery } from "@tanstack/react-query";
import { RANGES } from "../../../constants";
import { useWindowFocus } from "../../../hooks/useWindowFocus";
import { useI18n } from "../../../i18n/useI18n";
import { effectivePreset } from "../../../lib/presets";
import { queries } from "../../../lib/queries";
import type { SectionProps } from "../../../types/ui";
import { FolderLink } from "../controls/FolderLink";
import { NumberField } from "../controls/NumberField";
import { PercentSlider } from "../controls/PercentSlider";
import { NotificationPreview } from "../NotificationPreview";
import { PresetPicker } from "../PresetPicker";
import { SettingRow } from "../SettingRow";
import { SwitchRow } from "../SwitchRow";

export function Notifications({ settings, update }: SectionProps) {
  const { t } = useI18n();
  const { data: presets, refetch } = useQuery(queries.presets());
  useWindowFocus(() => void refetch());
  const current = effectivePreset(presets ?? [], settings.preset);

  return (
    <>
      <div className="flex flex-col gap-4 border-line border-b py-3.5">
        <NotificationPreview
          preset={current}
          position={settings.position}
          scale={settings.scale}
          onPosition={(position) => void update("position", position)}
        />
        <PresetPicker
          presets={presets ?? []}
          selected={current?.id}
          onSelect={(id) => void update("preset", id)}
        />
        <FolderLink folder="presets" label={t("settings.openPresets")} />
      </div>
      <SettingRow label={t("settings.size")}>
        <PercentSlider
          label={t("settings.size")}
          value={settings.scale}
          range={RANGES.scale}
          onCommit={(value) => void update("scale", value)}
        />
      </SettingRow>
      <SettingRow label={t("settings.rareBelow")}>
        <NumberField
          label={t("settings.rareBelow")}
          value={settings.rareThreshold}
          range={RANGES.rareThreshold}
          suffix={t("settings.ofPlayers")}
          onCommit={(value) => void update("rareThreshold", value)}
        />
      </SettingRow>
      <SwitchRow
        label={t("settings.progressPopups")}
        checked={settings.progressPopups}
        onChange={(value) => void update("progressPopups", value)}
      />
      <SwitchRow
        label={t("settings.steamPopups")}
        hint={t("settings.steamPopupsHint")}
        checked={settings.steamPopups}
        onChange={(value) => void update("steamPopups", value)}
      />
    </>
  );
}
