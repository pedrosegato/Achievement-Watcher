import { Button } from "../../../components/ui/button";
import { useI18n } from "../../../i18n/useI18n";
import { isSynced, panelTitleKey, statusSummary } from "../../../lib/cloud";
import { api } from "../../../lib/commands";
import { formatWhen } from "../../../lib/format";
import type { CloudGameView } from "../../../types/domain";
import type { Translate } from "../../../types/ui";
import { SummaryMark } from "./SummaryMark";
import type { CloudPanelProps } from "./types";

function summaryLines(
  view: CloudGameView,
  t: Translate,
  when: (at: number) => string,
): { title: string; details: string; retry: boolean } {
  const live = statusSummary(view);
  if (!live) {
    const synced = isSynced(view);
    return {
      title: t(panelTitleKey(view), { device: view.openElsewhere ?? "" }),
      details: synced ? `${when(view.lastSnapshotAt)}.` : "",
      retry: false,
    };
  }
  const lines = [
    live.at !== null ? `${when(live.at)}.` : null,
    live.detail ? t(live.detail.key, live.detail.vars) : null,
  ];
  return {
    title: t(live.title.key, live.title.vars),
    details: lines.filter(Boolean).join(" "),
    retry: live.retry,
  };
}

export function CloudSummary({ appId, view, now, busy, run }: CloudPanelProps & { now: number }) {
  const { t, locale } = useI18n();
  const { title, details, retry } = summaryLines(view, t, (at) => formatWhen(at, locale, now));
  const reason =
    !view.enabled && view.default !== "on" ? t(`cloud.card.default.${view.default}`) : null;
  const undo = view.lastAction?.canUndo === true;

  return (
    <div className="flex items-start gap-3 px-6 pt-4 pb-4">
      <SummaryMark view={view} />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <p className="font-semibold text-[15px] leading-snug">{title}</p>
        {details && <p className="text-[13px] text-muted">{details}</p>}
        {reason && <p className="text-[13px] text-muted">{reason}</p>}
      </div>
      {retry && (
        <Button
          size="sm"
          disabled={busy}
          onClick={() => void run(() => api.cloudSyncNow(appId), { queued: true })}
        >
          {t("cloud.status.retry")}
        </Button>
      )}
      {undo && (
        <Button
          size="sm"
          disabled={busy}
          aria-label={t("cloud.card.undoLabel")}
          onClick={() => void run(() => api.cloudUndo(appId), { queued: true })}
        >
          {t("cloud.card.undo")}
        </Button>
      )}
    </div>
  );
}
