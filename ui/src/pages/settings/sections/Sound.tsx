import { RANGES } from "../../../constants";
import { useI18n } from "../../../i18n/useI18n";
import type { SectionProps } from "../../../types/ui";
import { FolderLink } from "../controls/FolderLink";
import { PercentSlider } from "../controls/PercentSlider";
import { SettingBlock } from "../SettingBlock";
import { SettingRow } from "../SettingRow";
import { SoundPacks } from "../SoundPacks";
import { SwitchRow } from "../SwitchRow";

export function Sound({ settings, update }: SectionProps) {
  const { t } = useI18n();
  return (
    <>
      <SwitchRow
        label={t("settings.soundEnable")}
        checked={settings.sound}
        onChange={(value) => void update("sound", value)}
      />
      <SettingRow label={t("settings.volume")}>
        <PercentSlider
          label={t("settings.volume")}
          value={settings.soundVolume}
          range={RANGES.volume}
          onCommit={(value) => void update("soundVolume", value)}
        />
      </SettingRow>
      <SettingBlock label={t("settings.soundPack")}>
        <SoundPacks selected={settings.soundId} onSelect={(id) => void update("soundId", id)} />
        <FolderLink folder="sounds" label={t("settings.openSounds")} />
      </SettingBlock>
    </>
  );
}
