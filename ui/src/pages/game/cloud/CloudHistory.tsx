import { memo, useMemo, useState } from "react";
import type { Run } from "../../../hooks/useCloudAction";
import { useI18n } from "../../../i18n/useI18n";
import { layoutGraph } from "../../../lib/graph";
import type { CloudGameView } from "../../../types/domain";
import { HistoryRow } from "./HistoryRow";

interface HistoryProps {
  appId: number;
  snapshots: CloudGameView["snapshots"];
  now: number;
  busy: boolean;
  run: Run;
}

export const CloudHistory = memo(function CloudHistory({
  appId,
  snapshots,
  now,
  busy,
  run,
}: HistoryProps) {
  const { t } = useI18n();
  const [chosen, setChosen] = useState<string | null>(null);
  const rows = useMemo(
    () =>
      layoutGraph(
        snapshots.map((snapshot) => ({
          id: snapshot.id,
          parent: snapshot.parent,
          createdAt: snapshot.createdAt,
        })),
      ),
    [snapshots],
  );
  const byId = useMemo(
    () => new Map(snapshots.map((snapshot) => [snapshot.id, snapshot])),
    [snapshots],
  );
  const lanes = Math.max(1, ...rows.map((row) => row.width));
  const branched = lanes > 1;

  return (
    <section className="flex min-h-0 flex-1 flex-col border-line border-t">
      <div className="flex items-baseline justify-between gap-4 px-6 pt-4 pb-2">
        <h3 className="font-semibold text-[14px]">{t("cloud.history.title")}</h3>
      </div>
      {branched && (
        <p className="px-6 pb-2 text-[12.5px] text-muted">{t("cloud.history.branches")}</p>
      )}
      {rows.length === 0 ? (
        <p className="mx-6 mb-4 rounded-card border border-line border-dashed p-3 text-[13px] text-muted">
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
                now={now}
                busy={busy}
                choosing={busy && chosen === row.id}
                onChoose={setChosen}
                pinned={branched}
                run={run}
              />
            ) : null;
          })}
        </ol>
      )}
    </section>
  );
});
