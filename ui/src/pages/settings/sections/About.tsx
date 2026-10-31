import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { AppLogo } from "../../../components/AppLogo";
import { Icon } from "../../../components/Icon";
import { Spinner } from "../../../components/Spinner";
import { Button } from "../../../components/ui/button";
import { APP_NAME, EVENTS } from "../../../constants";
import { useTauriEvent } from "../../../hooks/useTauriEvent";
import { useI18n } from "../../../i18n/useI18n";
import { api, errorMessage } from "../../../lib/commands";
import { queries } from "../../../lib/queries";
import { updateView } from "../../../lib/update";
import type { UpdateStatus } from "../../../types/domain";
import type { SectionProps } from "../../../types/ui";
import { DiagnosticsSections } from "../DiagnosticsSections";
import { SettingBlock } from "../SettingBlock";
import { SwitchRow } from "../SwitchRow";

export function About({ settings, update }: SectionProps) {
  const { t } = useI18n();
  const client = useQueryClient();
  const query = queries.updateStatus();
  const { data: status } = useQuery(query);
  const [exported, setExported] = useState<string | null>(null);

  const setStatus = (next: UpdateStatus) => client.setQueryData(query.queryKey, next);
  useTauriEvent<UpdateStatus>(EVENTS.updateProgress, setStatus);

  const start = (run: () => Promise<void>) => {
    if (status) setStatus({ ...status, phase: "checking", error: null });
    run().catch(() => undefined);
  };
  const exportDiagnostics = () => {
    api
      .exportDiagnostics()
      .then((path) => setExported(t("settings.exported", { path })))
      .catch((reason: unknown) => setExported(errorMessage(reason)));
  };

  const view = status ? updateView(status) : null;

  return (
    <>
      <div className="flex items-center gap-3.5 border-line border-b pt-1.5 pb-4">
        <AppLogo className="size-11" />
        <div className="mr-auto flex flex-col">
          <span className="font-bold text-[16px]">{APP_NAME}</span>
          <span className="text-[12.5px] text-dim" aria-live="polite">
            {view?.note && t(view.note.key, view.note.vars)}
          </span>
        </div>
        {view && (
          <Button
            variant={status?.available ? "primary" : "secondary"}
            disabled={view.busy}
            aria-busy={view.busy}
            onClick={() => start(status?.available ? api.installUpdate : api.checkUpdate)}
            className="min-w-44 tabular-nums"
          >
            {view.busy ? (
              <Spinner />
            ) : (
              <Icon name={status?.available ? "download" : "sync"} size={16} />
            )}
            {t(view.action.key, view.action.vars)}
          </Button>
        )}
      </div>
      {view?.percent != null && (
        <div
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={view.percent}
          className="-mt-px h-0.5 overflow-hidden bg-line"
        >
          <div
            className="h-full bg-text transition-[width] duration-(--dur-med) ease-(--ease-out)"
            style={{ width: `${view.percent}%` }}
          />
        </div>
      )}
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
