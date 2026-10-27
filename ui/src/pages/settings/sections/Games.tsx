import { useQuery } from "@tanstack/react-query";
import { Icon } from "../../../components/Icon";
import { Button } from "../../../components/ui/button";
import { useScan } from "../../../hooks/useScan";
import { useI18n } from "../../../i18n/useI18n";
import { queries } from "../../../lib/queries";
import type { SectionProps } from "../../../types/ui";
import { FolderList } from "../FolderList";
import { SettingBlock } from "../SettingBlock";
import { SettingRow } from "../SettingRow";
import { SwitchRow } from "../SwitchRow";

export function Games({ settings, update }: SectionProps) {
  const { t } = useI18n();
  const { scanning, start } = useScan();
  const { data: diagnostics } = useQuery(queries.diagnostics());
  const emulators = [
    ...new Set(
      (diagnostics?.sources ?? []).filter((source) => source.exists).map((source) => source.name),
    ),
  ];
  const busy = scanning !== null;

  return (
    <>
      <SwitchRow
        label={t("settings.readSteam")}
        checked={settings.readSteamLibraries}
        onChange={(value) => void update("readSteamLibraries", value)}
      />
      <SwitchRow
        label={t("settings.steamAchievements")}
        checked={settings.steamAchievements}
        onChange={(value) => void update("steamAchievements", value)}
      />
      <SettingRow label={t("library.rescan")}>
        <Button size="sm" onClick={busy ? undefined : start} aria-busy={busy}>
          <Icon name="sync" size={16} className={busy ? "spinning" : undefined} />
          {busy ? t("library.scanning", { pct: scanning }) : t("settings.rescan")}
        </Button>
      </SettingRow>
      <SettingRow label={t("settings.emulators")}>
        <span className="flex flex-wrap justify-end gap-1.5">
          {emulators.length === 0 && (
            <span className="text-[13px] text-dim">{t("settings.diag.none")}</span>
          )}
          {emulators.map((name) => (
            <span
              key={name}
              className="inline-flex items-center gap-[5px] rounded-card border border-line bg-surface px-[9px] py-1 font-semibold text-[12.5px]"
            >
              <Icon name="check" size={14} className="text-muted" />
              {name}
            </span>
          ))}
        </span>
      </SettingRow>
      <SettingBlock label={t("settings.extraFolders")}>
        <FolderList
          label={t("settings.extraFolders")}
          values={settings.extraGameRoots}
          onChange={(values) => void update("extraGameRoots", values)}
        />
      </SettingBlock>
      <SettingBlock label={t("settings.gameFolders")}>
        <FolderList
          label={t("settings.gameFolders")}
          values={settings.extraGameDirs}
          onChange={(values) => void update("extraGameDirs", values)}
        />
      </SettingBlock>
    </>
  );
}
