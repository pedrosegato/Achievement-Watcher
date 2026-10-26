import { memo, type ReactNode } from "react";
import { AchievementIcon } from "../../../components/AchievementIcon";
import { useI18n } from "../../../i18n/useI18n";
import { cn } from "../../../lib/cn";
import { formatDateTime, formatPercent } from "../../../lib/format";
import type { AchievementRow } from "../../../types/domain";

interface DialogRowProps {
  row: AchievementRow;
  rare?: boolean;
  locked?: boolean;
  aside?: ReactNode;
}

export const DialogRow = memo(function DialogRow({
  row,
  rare = false,
  locked = false,
  aside,
}: DialogRowProps) {
  const { t, locale } = useI18n();
  const side =
    aside ??
    (row.unlockedAt !== null && (
      <time className="text-[12px] text-muted @md:whitespace-nowrap">
        {t("dialog.unlockedOn", { date: formatDateTime(row.unlockedAt, locale) })}
      </time>
    ));
  return (
    <div className="dialog-row @container rounded-card border border-line bg-surface">
      <div className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-3.5 gap-y-1.5 px-3.5 py-2.5 @md:grid-cols-[auto_minmax(0,1fr)_auto]">
        <AchievementIcon
          src={locked ? (row.iconGrayUrl ?? row.iconUrl) : row.iconUrl}
          gray={locked}
          rare={rare}
          size={40}
          className={cn(side && "row-span-2 @md:row-span-1")}
        />
        <div className="flex min-w-0 flex-col">
          <span className={cn("line-clamp-2 font-[650] text-[14px]", locked && "text-muted")}>
            {row.displayName}
          </span>
          <span className="text-[12.5px] text-muted">{row.description}</span>
          {row.percent !== null && (
            <span className="text-[12px] text-dim">
              {t("achievement.playersOf", { pct: formatPercent(row.percent, locale) })}
            </span>
          )}
        </div>
        {side && <div className="col-start-2 @md:col-start-3 @md:row-start-1">{side}</div>}
      </div>
    </div>
  );
});
