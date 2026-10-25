import { useDeferredValue } from "react";
import { useI18n } from "../../../i18n/useI18n";
import { filterDialog, isRare, limitSplit, splitLocked } from "../../../lib/achievements";
import { useRareThreshold } from "../../../settings/useAppSettings";
import type { AchievementFilter, AchievementRow } from "../../../types/domain";
import { DialogRow } from "./DialogRow";
import { HiddenRow } from "./HiddenRow";
import type { HiddenAction } from "./hidden";

const FIRST_PAINT_ROWS = 16;

interface AchievementListProps {
  rows: AchievementRow[];
  filter: AchievementFilter;
  query?: string;
  maskHidden: boolean;
  revealed: ReadonlySet<string>;
  dispatch: (action: HiddenAction) => void;
}

function GroupTitle({ children }: { children: string }) {
  return <h3 className="mt-2.5 mb-0.5 font-semibold text-[12.5px] text-muted">{children}</h3>;
}

export function AchievementList({
  rows,
  filter,
  query = "",
  maskHidden,
  revealed,
  dispatch,
}: AchievementListProps) {
  const { t } = useI18n();
  const threshold = useRareThreshold();
  const complete = useDeferredValue(true, false);
  const groups = limitSplit(
    splitLocked(filterDialog(rows, filter, query, maskHidden), maskHidden),
    complete ? Number.POSITIVE_INFINITY : FIRST_PAINT_ROWS,
  );
  const empty = groups.unlocked.length + groups.locked.length + groups.hidden.length === 0;
  const showTitles = filter === "all";

  return (
    <>
      {empty && <p className="py-8 text-center text-[13px] text-dim">{t("dialog.empty")}</p>}
      {groups.unlocked.length > 0 && showTitles && <GroupTitle>{t("game.unlocked")}</GroupTitle>}
      {groups.unlocked.map((row) => (
        <DialogRow key={row.apiName} row={row} rare={isRare(row.percent, threshold)} />
      ))}
      {groups.locked.length > 0 && showTitles && <GroupTitle>{t("game.locked")}</GroupTitle>}
      {groups.locked.map((row) => (
        <DialogRow key={row.apiName} row={row} locked />
      ))}
      {groups.hidden.length > 0 && <GroupTitle>{t("dialog.hidden")}</GroupTitle>}
      {groups.hidden.map((row) => (
        <HiddenRow
          key={row.apiName}
          row={row}
          revealed={revealed.has(row.apiName)}
          dispatch={dispatch}
        />
      ))}
    </>
  );
}
