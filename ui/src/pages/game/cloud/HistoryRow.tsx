import { Spinner } from "../../../components/Spinner";
import { Button } from "../../../components/ui/button";
import type { Run } from "../../../hooks/useCloudAction";
import { useI18n } from "../../../i18n/useI18n";
import { playtimeLabel } from "../../../lib/cloud";
import { api } from "../../../lib/commands";
import { formatHours, formatWhen } from "../../../lib/format";
import type { GraphRow } from "../../../lib/graph";
import type { SnapshotView } from "../../../types/domain";
import { HistoryLanes } from "./HistoryLanes";
import { laneX, ROW } from "./history";

interface HistoryRowProps {
  appId: number;
  row: GraphRow;
  lanes: number;
  snapshot: SnapshotView;
  now: number;
  busy: boolean;
  choosing: boolean;
  onChoose: (id: string) => void;
  pinned: boolean;
  run: Run;
}

const USE_HIDDEN =
  "flex-none opacity-0 transition-opacity duration-(--dur-fast) ease-(--ease-out) focus-visible:opacity-100 group-hover:opacity-100 disabled:invisible";

const DOT_TARGET =
  "-translate-x-1/2 -translate-y-1/2 absolute size-6 cursor-pointer rounded-full outline-none transition-shadow duration-(--dur-fast) ease-(--ease-out) hover:ring-2 hover:ring-text/50 focus-visible:ring-2 focus-visible:ring-text disabled:cursor-default disabled:ring-0";

export function HistoryRow({
  appId,
  row,
  lanes,
  snapshot,
  now,
  busy,
  choosing,
  onChoose,
  pinned,
  run,
}: HistoryRowProps) {
  const { t, locale } = useI18n();
  const device = snapshot.mine ? t("cloud.history.thisPc") : snapshot.deviceName;
  const hours = playtimeLabel(snapshot.playtimeSeconds);
  const when = formatWhen(snapshot.createdAt, locale, now);
  const choose = () => {
    onChoose(snapshot.id);
    void run(() => api.cloudSetPrincipal(appId, snapshot.id), { queued: true });
  };

  return (
    <li className="group flex items-center gap-3" style={{ height: ROW }}>
      <div className="relative flex-none">
        <HistoryLanes row={row} lanes={lanes} principal={snapshot.isPrincipal} />
        {choosing && (
          <span
            className="-translate-x-1/2 -translate-y-1/2 absolute grid size-5 place-items-center rounded-full bg-raised text-text"
            style={{ left: laneX(row.lane), top: ROW / 2 }}
          >
            <Spinner className="size-3.5" />
          </span>
        )}
        {!snapshot.isPrincipal && !choosing && (
          <button
            type="button"
            aria-label={t("cloud.history.useAt", { time: when })}
            disabled={busy}
            onClick={choose}
            className={DOT_TARGET}
            style={{ left: laneX(row.lane), top: ROW / 2 }}
          />
        )}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate font-semibold text-[13.5px]">{when}</span>
          {snapshot.isPrincipal && (
            <span className="flex-none rounded-tag bg-text px-1.5 py-px font-semibold text-[11px] text-bg">
              {t("cloud.history.current")}
            </span>
          )}
          {snapshot.foundOnDevice && (
            <span className="flex-none rounded-tag border border-line px-1.5 py-px text-[11px] text-muted">
              {t("cloud.history.found")}
            </span>
          )}
        </div>
        <div className="flex min-w-0 gap-3 text-[12.5px] text-muted">
          <span className="truncate">{device}</span>
          {hours && (
            <span className="flex-none">
              {hours.key === "cloud.history.hours"
                ? t(hours.key, { hours: formatHours(hours.seconds, locale) })
                : t(hours.key)}
            </span>
          )}
          {snapshot.unlocked !== null && (
            <span className="flex-none">
              {t("cloud.history.unlocked", { count: snapshot.unlocked })}
            </span>
          )}
        </div>
      </div>
      {choosing && (
        <span
          role="status"
          className="flex flex-none items-center gap-2 px-3 text-[12.5px] text-muted"
        >
          <Spinner className="size-3.5" />
          {t("cloud.history.choosing")}
        </span>
      )}
      {!snapshot.isPrincipal && !choosing && (
        <Button
          size="sm"
          variant={pinned ? "secondary" : "ghost"}
          disabled={busy}
          className={pinned ? "flex-none" : USE_HIDDEN}
          onClick={choose}
        >
          {t("cloud.history.use")}
        </Button>
      )}
    </li>
  );
}
