import { useId, useMemo } from "react";
import { ErrorAlert } from "../../components/ErrorAlert";
import { Icon } from "../../components/Icon";
import { Button } from "../../components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "../../components/ui/dialog";
import { Switch } from "../../components/ui/switch";
import { useCloudAction } from "../../hooks/useCloudAction";
import type { Key } from "../../i18n/dictionary";
import { useI18n } from "../../i18n/useI18n";
import {
  elidedPath,
  isSynced,
  lastActionKey,
  panelTitleKey,
  playtimeLabel,
  savePathAction,
} from "../../lib/cloud";
import { api } from "../../lib/commands";
import { pickFolder } from "../../lib/dialog";
import { formatHours, formatWhen } from "../../lib/format";
import { type GraphRow, layoutGraph } from "../../lib/graph";
import type { CloudGameView, SnapshotView } from "../../types/domain";

const LOCATED_KEYS = {
  notFound: "cloud.card.notFound",
  registry: "cloud.card.registry",
  unsupported: "cloud.card.unsupported",
  waitingManifest: "cloud.card.waitingManifest",
} as const;

const LANE = 18;
const ROW = 60;
const DOT = 5;

type Run = (work: () => Promise<void>) => Promise<void>;

interface CloudDialogProps {
  appId: number;
  gameName: string;
  view: CloudGameView;
  thisPc: string;
  now: number;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function CloudDialog({
  appId,
  gameName,
  view,
  thisPc,
  now,
  open,
  onOpenChange,
}: CloudDialogProps) {
  const { t } = useI18n();
  const { error, run } = useCloudAction();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[min(92vw,640px)]">
        <div className="flex items-start justify-between gap-4 border-line border-b px-6 pt-5 pb-4">
          <div className="flex min-w-0 flex-col gap-0.5">
            <DialogTitle className="font-bold text-[18px]">{t("cloud.card.title")}</DialogTitle>
            <DialogDescription className="truncate text-[13px] text-dim">
              {gameName}
            </DialogDescription>
          </div>
          <DialogClose asChild>
            <button
              type="button"
              aria-label={t("window.close")}
              className="grid size-8 flex-none cursor-pointer place-items-center rounded-card text-muted transition-colors duration-(--dur-fast) ease-(--ease-out) hover:bg-surface-2 hover:text-text"
            >
              <Icon name="close" size={16} />
            </button>
          </DialogClose>
        </div>
        <Summary appId={appId} view={view} thisPc={thisPc} now={now} run={run} />
        {error && (
          <div className="px-6 pb-3">
            <ErrorAlert message={error} />
          </div>
        )}
        <History appId={appId} view={view} thisPc={thisPc} now={now} run={run} />
        <SaveFolder appId={appId} view={view} run={run} />
        <AutoSync appId={appId} view={view} run={run} />
      </DialogContent>
    </Dialog>
  );
}

interface PanelProps {
  appId: number;
  view: CloudGameView;
  thisPc: string;
  now: number;
  run: Run;
}

function actionLine(
  view: CloudGameView,
  thisPc: string,
  t: (key: Key, vars?: Record<string, string>) => string,
): string | null {
  const action = view.lastAction;
  if (!action) return null;
  if (action.reason === "push" && action.deviceName === thisPc) {
    return t("cloud.card.lastAction.pushHere");
  }
  return t(lastActionKey(action.reason), { device: action.deviceName });
}

function Summary({ appId, view, thisPc, now, run }: PanelProps) {
  const { t, locale } = useI18n();
  const synced = isSynced(view);
  const details = [
    synced ? formatWhen(view.lastSnapshotAt, locale, now) : null,
    synced ? actionLine(view, thisPc, t) : null,
  ]
    .filter(Boolean)
    .map((line) => `${line}.`)
    .join(" ");
  const reason =
    !view.enabled && view.default !== "on" ? t(`cloud.card.default.${view.default}`) : null;

  return (
    <div className="flex items-start gap-3 px-6 pt-4 pb-4">
      <span
        className={
          synced
            ? "grid size-9 flex-none place-items-center rounded-full bg-surface-2 text-text"
            : "grid size-9 flex-none place-items-center rounded-full bg-surface-2 text-dim"
        }
      >
        <Icon name={synced ? "check" : "cloud"} size={18} />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <p className="font-semibold text-[15px] leading-snug">
          {t(panelTitleKey(view), { device: view.openElsewhere ?? "" })}
        </p>
        {details && <p className="text-[13px] text-dim">{details}</p>}
        {reason && <p className="text-[13px] text-dim">{reason}</p>}
      </div>
      {view.lastAction?.canUndo && (
        <Button size="sm" onClick={() => void run(() => api.cloudUndo(appId))}>
          {t("cloud.card.undo")}
        </Button>
      )}
    </div>
  );
}

function History({ appId, view, thisPc, now, run }: PanelProps) {
  const { t } = useI18n();
  const rows = useMemo(
    () =>
      layoutGraph(
        view.snapshots.map((snapshot) => ({
          id: snapshot.id,
          parent: snapshot.parent,
          createdAt: snapshot.createdAt,
        })),
      ),
    [view.snapshots],
  );
  const byId = useMemo(
    () => new Map(view.snapshots.map((snapshot) => [snapshot.id, snapshot])),
    [view.snapshots],
  );
  const lanes = Math.max(1, ...rows.map((row) => row.width));
  const branched = lanes > 1;

  return (
    <section className="flex min-h-0 flex-1 flex-col border-line border-t">
      <div className="flex items-baseline justify-between gap-4 px-6 pt-4 pb-2">
        <h3 className="font-semibold text-[14px]">{t("cloud.history.title")}</h3>
      </div>
      {branched && (
        <p className="px-6 pb-2 text-[12.5px] text-dim">{t("cloud.history.branches")}</p>
      )}
      {rows.length === 0 ? (
        <p className="mx-6 mb-4 rounded-card border border-line border-dashed p-3 text-[13px] text-dim">
          {t("cloud.history.empty")}
        </p>
      ) : (
        <ol className="min-h-0 flex-1 overflow-y-auto px-6 pb-3">
          {rows.map((row) => {
            const snapshot = byId.get(row.id);
            return snapshot ? (
              <HistoryRow
                key={row.id}
                appId={appId}
                row={row}
                lanes={lanes}
                snapshot={snapshot}
                thisPc={thisPc}
                now={now}
                run={run}
              />
            ) : null;
          })}
        </ol>
      )}
    </section>
  );
}

interface HistoryRowProps {
  appId: number;
  row: GraphRow;
  lanes: number;
  snapshot: SnapshotView;
  thisPc: string;
  now: number;
  run: Run;
}

function HistoryRow({ appId, row, lanes, snapshot, thisPc, now, run }: HistoryRowProps) {
  const { t, locale } = useI18n();
  const device = snapshot.deviceName === thisPc ? t("cloud.history.thisPc") : snapshot.deviceName;
  const hours = playtimeLabel(snapshot.playtimeSeconds);

  return (
    <li className="group flex items-center gap-3" style={{ height: ROW }}>
      <Lanes row={row} lanes={lanes} principal={snapshot.isPrincipal} />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate font-semibold text-[13.5px]">
            {formatWhen(snapshot.createdAt, locale, now)}
          </span>
          {snapshot.isPrincipal && (
            <span className="flex-none rounded-tag bg-text px-1.5 py-px font-semibold text-[11px] text-bg">
              {t("cloud.history.current")}
            </span>
          )}
          {snapshot.foundOnDevice && (
            <span className="flex-none rounded-tag border border-line px-1.5 py-px text-[11px] text-dim">
              {t("cloud.history.found")}
            </span>
          )}
        </div>
        <div className="flex min-w-0 gap-3 text-[12.5px] text-dim">
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
      {!snapshot.isPrincipal && (
        <Button
          size="sm"
          variant="ghost"
          className="flex-none opacity-0 transition-opacity duration-(--dur-fast) ease-(--ease-out) focus-visible:opacity-100 group-hover:opacity-100"
          onClick={() => void run(() => api.cloudSetPrincipal(appId, snapshot.id))}
        >
          {t("cloud.history.use")}
        </Button>
      )}
    </li>
  );
}

function laneX(lane: number): number {
  return lane * LANE + LANE / 2;
}

function Lanes({ row, lanes, principal }: { row: GraphRow; lanes: number; principal: boolean }) {
  const width = lanes * LANE;
  const x = laneX(row.lane);
  const middle = ROW / 2;

  return (
    <svg
      width={width}
      height={ROW}
      viewBox={`0 0 ${width} ${ROW}`}
      className="flex-none text-dim/60"
      aria-hidden="true"
    >
      {row.lanesAbove.map((lane) => (
        <line
          key={`pass-${lane}`}
          x1={laneX(lane)}
          x2={laneX(lane)}
          y1={0}
          y2={ROW}
          stroke="currentColor"
          strokeWidth={2}
        />
      ))}
      {row.merges.map((lane) => (
        <path
          key={`merge-${lane}`}
          d={`M ${laneX(lane)} 0 C ${laneX(lane)} ${middle * 0.6}, ${x} ${middle * 0.4}, ${x} ${middle}`}
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
        />
      ))}
      {row.incoming && (
        <line x1={x} x2={x} y1={0} y2={middle} stroke="currentColor" strokeWidth={2} />
      )}
      {row.continues && (
        <line x1={x} x2={x} y1={middle} y2={ROW} stroke="currentColor" strokeWidth={2} />
      )}
      <circle
        cx={x}
        cy={middle}
        r={principal ? DOT + 1 : DOT}
        className={principal ? "fill-text stroke-raised" : "fill-raised stroke-dim"}
        strokeWidth={2}
      />
    </svg>
  );
}

function SaveFolder({ appId, view, run }: { appId: number; view: CloudGameView; run: Run }) {
  const { t } = useI18n();
  const action = savePathAction(view.located);
  const path = view.paths[0] ?? null;
  const browse = () =>
    void run(async () => {
      const picked = await pickFolder(t("cloud.panel.saveFolder"), path ?? undefined);
      if (picked) await api.setGameSavePath(appId, picked);
    });

  return (
    <div className="flex flex-col gap-1 border-line border-t px-6 py-3.5">
      {action === "change" && path ? (
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 flex-col gap-0.5">
            <span className="text-[12px] text-dim">{t("cloud.panel.saveFolder")}</span>
            <button
              type="button"
              title={`${t("cloud.panel.openFolder")}: ${path}`}
              onClick={() => void api.openPath(path)}
              className="min-w-0 cursor-pointer truncate whitespace-nowrap text-left font-medium text-[13px] underline-offset-[3px] outline-none hover:underline focus-visible:underline"
            >
              {elidedPath(path)}
            </button>
          </div>
          <Button size="sm" onClick={browse}>
            {t("cloud.change")}
          </Button>
        </div>
      ) : (
        <>
          <span className="text-[12px] text-dim">{t("cloud.panel.saveFolder")}</span>
          {view.located !== "found" && (
            <p className="text-[13px] text-dim">{t(LOCATED_KEYS[view.located])}</p>
          )}
          {action === "choose" && (
            <Button size="sm" variant="primary" className="w-max" onClick={browse}>
              <Icon name="folder-open" size={16} />
              {t("cloud.card.chooseFolder")}
            </Button>
          )}
        </>
      )}
      {view.oversize && <p className="text-[12.5px] text-muted">{t("cloud.card.oversize")}</p>}
    </div>
  );
}

function AutoSync({ appId, view, run }: { appId: number; view: CloudGameView; run: Run }) {
  const { t } = useI18n();
  const id = useId();
  if (!view.key) return null;

  return (
    <div className="flex items-center justify-between gap-3 border-line border-t px-6 py-3.5">
      <label htmlFor={id} className="cursor-pointer font-medium text-[13px]">
        {t("cloud.panel.autoSync")}
      </label>
      <Switch
        id={id}
        checked={view.enabled}
        onCheckedChange={(enabled) => void run(() => api.setGameSync(appId, enabled))}
      />
    </div>
  );
}
