import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Collapsible } from "../../components/Collapsible";
import { useI18n } from "../../i18n/useI18n";
import { queries } from "../../lib/queries";

type Section = "emulators" | "files" | "unresolved" | "log";

const SHOWN_FILES = 8;
const STEAM = "steam";

export function DiagnosticsSections() {
  const { t } = useI18n();
  const { data } = useQuery(queries.diagnostics());
  const { data: games } = useQuery(queries.games());
  const [open, setOpen] = useState<ReadonlySet<Section>>(() => new Set());

  if (!data) return null;

  const toggle = (section: Section) =>
    setOpen((current) => {
      const next = new Set(current);
      if (!next.delete(section)) next.add(section);
      return next;
    });
  const names = new Map((games ?? []).map((game) => [game.appId, game.name]));
  const sourceNames = new Map(data.sources.map((source) => [source.id, source.name]));
  const sourceName = (id: string) => (id === STEAM ? "Steam" : (sourceNames.get(id) ?? id));
  const emulators = data.sources.filter((source) => source.exists && source.root);
  const files = [...data.files].sort((a, b) => (b.last_ok_at ?? 0) - (a.last_ok_at ?? 0));
  const extra = files.length - SHOWN_FILES;
  const line = "flex flex-wrap items-baseline gap-x-2.5 py-1 text-[12.5px]";

  return (
    <div className="diag-sections flex flex-col">
      <Collapsible
        open={open.has("emulators")}
        onToggle={() => toggle("emulators")}
        header={t("settings.diag.emulators")}
        count={emulators.length}
      >
        {emulators.length === 0 && <p className={line}>{t("settings.diag.none")}</p>}
        {emulators.map((source) => (
          <p key={source.id} className={line}>
            <b className="font-semibold">{source.name}</b>
            <span className="break-all text-muted">{source.root}</span>
          </p>
        ))}
      </Collapsible>
      <Collapsible
        open={open.has("files")}
        onToggle={() => toggle("files")}
        header={t("settings.diag.files")}
        count={files.length}
      >
        {files.slice(0, SHOWN_FILES).map((file) => (
          <p key={file.path} className={line}>
            <b className="font-semibold">
              {names.get(file.app_id) ?? t("settings.diag.app", { id: file.app_id })}
            </b>
            <span className="text-dim">{sourceName(file.source_id)}</span>
            {file.last_count !== null && (
              <span className="text-muted">{t("settings.diag.count", { n: file.last_count })}</span>
            )}
            {file.last_error && <em className="text-muted not-italic">{file.last_error}</em>}
          </p>
        ))}
        {extra > 0 && <p className={`${line} text-dim`}>{t("settings.diag.more", { n: extra })}</p>}
      </Collapsible>
      <Collapsible
        open={open.has("unresolved")}
        onToggle={() => toggle("unresolved")}
        header={t("settings.diag.unresolved")}
        count={data.unresolved.length}
      >
        {data.unresolved.length === 0 && <p className={line}>{t("settings.diag.nothing")}</p>}
        {data.unresolved.map((item) => (
          <p key={item.path} className={line}>
            <b className="font-semibold">{item.reason}</b>
            <span className="break-all text-muted">{item.path}</span>
          </p>
        ))}
      </Collapsible>
      <Collapsible
        open={open.has("log")}
        onToggle={() => toggle("log")}
        header={t("settings.diag.log")}
        count={t("settings.diag.lastLines")}
      >
        <p className="whitespace-pre-wrap break-all py-0.5 text-[12px] text-muted leading-relaxed">
          {data.logTail.join("\n")}
        </p>
      </Collapsible>
    </div>
  );
}
