import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { AppLogo } from "../../../components/AppLogo";
import { Icon } from "../../../components/Icon";
import { Button } from "../../../components/ui/button";
import { APP_NAME, TIMINGS } from "../../../constants";
import { useI18n } from "../../../i18n/useI18n";
import { api, errorMessage } from "../../../lib/commands";
import { queries } from "../../../lib/queries";
import type { SectionProps } from "../../../types/ui";
import { DiagnosticsSections } from "../DiagnosticsSections";
import { SettingBlock } from "../SettingBlock";
import { SwitchRow } from "../SwitchRow";

export function About({ settings, update }: SectionProps) {
  const { t } = useI18n();
  const { data: status, refetch } = useQuery(queries.updateStatus());
  const [exported, setExported] = useState<string | null>(null);

  const check = () => {
    api
      .checkUpdate()
      .then(() => window.setTimeout(() => void refetch(), TIMINGS.updateRecheckMs))
      .catch(() => undefined);
  };
  const install = () => {
    api.installUpdate().catch(() => undefined);
  };
  const exportDiagnostics = () => {
    api
      .exportDiagnostics()
      .then((path) => setExported(t("settings.exported", { path })))
      .catch((reason: unknown) => setExported(errorMessage(reason)));
  };

  let version = "";
  if (status?.error) version = status.error;
  else if (status?.checkedAt && !status.available) {
    version = t("settings.versionCurrent", { version: status.current });
  } else if (status) version = t("settings.version", { version: status.current });

  return (
    <>
      <div className="flex items-center gap-3.5 border-line border-b pt-1.5 pb-4">
        <AppLogo className="size-11" />
        <div className="mr-auto flex flex-col">
          <span className="font-bold text-[16px]">{APP_NAME}</span>
          <span className="text-[12.5px] text-dim">{version}</span>
        </div>
        {status?.available ? (
          <Button variant="primary" onClick={install}>
            {t("settings.installUpdate", { version: status.available })}
          </Button>
        ) : (
          <Button onClick={check} aria-busy={status?.busy}>
            <Icon name="sync" size={16} className={status?.busy ? "spinning" : undefined} />
            {t("settings.checkUpdate")}
          </Button>
        )}
      </div>
      <SwitchRow
        label={t("settings.autoUpdate")}
        hint={t("settings.autoUpdateHint")}
        checked={settings.autoUpdate}
        onChange={(value) => void update("autoUpdate", value)}
      />
      <SettingBlock label={t("settings.diagnostics")}>
        <DiagnosticsSections />
        <div className="flex flex-wrap gap-2 pt-1">
          <Button
            size="sm"
            onClick={() => {
              api.openUserFolder("data").catch(() => undefined);
            }}
          >
            <Icon name="folder-open" size={16} />
            {t("settings.dataFolder")}
          </Button>
          <Button size="sm" onClick={exportDiagnostics}>
            <Icon name="download" size={16} />
            {t("settings.exportDiagnostics")}
          </Button>
        </div>
        {exported && <p className="break-all text-[12.5px] text-muted">{exported}</p>}
      </SettingBlock>
      <p className="pt-4 text-[12.5px] text-dim">{t("settings.credits")}</p>
    </>
  );
}
