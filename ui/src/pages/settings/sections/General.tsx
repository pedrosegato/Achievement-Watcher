import { useQuery } from "@tanstack/react-query";
import { Segmented } from "../../../components/Segmented";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../../../components/ui/select";
import { ACHIEVEMENT_LANGUAGES, AUTO_LANGUAGE, THEMES } from "../../../constants";
import type { Key } from "../../../i18n/dictionary";
import { useI18n } from "../../../i18n/useI18n";
import { steamLanguageName } from "../../../lib/languages";
import { queries } from "../../../lib/queries";
import type { Theme } from "../../../types/domain";
import type { SectionProps } from "../../../types/ui";
import { HotkeyField } from "../HotkeyField";
import { SettingRow } from "../SettingRow";
import { SwitchRow } from "../SwitchRow";

const SAME_AS_WINDOWS = "windows";

const THEME_LABELS: Record<Theme, Key> = {
  system: "settings.theme.system",
  light: "settings.theme.light",
  dark: "settings.theme.dark",
};
const THEME_ICONS = { system: "monitor", light: "sun", dark: "moon" } as const;

export function General({ settings, revision, update }: SectionProps) {
  const { t, locale } = useI18n();
  const { data: locales } = useQuery(queries.locales());
  const { data: hotkeyError } = useQuery(queries.hotkeyError(revision));

  return (
    <>
      <SettingRow label={t("settings.theme")}>
        <Segmented
          label={t("settings.theme")}
          value={settings.theme}
          onChange={(theme) => void update("theme", theme)}
          options={THEMES.map((theme) => ({
            value: theme,
            label: t(THEME_LABELS[theme]),
            icon: THEME_ICONS[theme],
          }))}
        />
      </SettingRow>
      <SettingRow label={t("settings.uiLanguage")}>
        <Select
          value={settings.uiLanguage}
          onValueChange={(value) => void update("uiLanguage", value)}
        >
          <SelectTrigger
            aria-label={t("settings.uiLanguage")}
            className="min-w-[170px] justify-between"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent align="end">
            <SelectItem value={AUTO_LANGUAGE}>{t("settings.sameAsWindows")}</SelectItem>
            {(locales ?? []).map((item) => (
              <SelectItem key={item.code} value={item.code}>
                {item.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </SettingRow>
      <SettingRow label={t("settings.achLanguage")}>
        <Select
          value={settings.language ?? SAME_AS_WINDOWS}
          onValueChange={(value) =>
            void update("language", value === SAME_AS_WINDOWS ? null : value)
          }
        >
          <SelectTrigger
            aria-label={t("settings.achLanguage")}
            className="min-w-[170px] justify-between"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent align="end">
            <SelectItem value={SAME_AS_WINDOWS}>{t("settings.sameAsWindows")}</SelectItem>
            {ACHIEVEMENT_LANGUAGES.map((language) => (
              <SelectItem key={language} value={language}>
                {steamLanguageName(language, locale)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </SettingRow>
      <SwitchRow
        label={t("settings.autostart")}
        checked={settings.autostart}
        onChange={(value) => void update("autostart", value)}
      />
      <SwitchRow
        label={t("settings.hideHidden")}
        hint={t("settings.hideHiddenHint")}
        checked={settings.hideHiddenAchievements}
        onChange={(value) => void update("hideHiddenAchievements", value)}
      />
      <SettingRow
        label={t("settings.overlayHotkey")}
        hint={hotkeyError ? t("settings.hotkeyError", { error: hotkeyError }) : undefined}
      >
        <HotkeyField
          value={settings.overlayHotkey}
          onChange={(combo) => void update("overlayHotkey", combo)}
        />
      </SettingRow>
    </>
  );
}
