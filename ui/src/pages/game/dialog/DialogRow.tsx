import type { ReactNode } from "react";
import { AchievementIcon } from "../../../components/AchievementIcon";
import { useI18n } from "../../../i18n/useI18n";
import { cn } from "../../../lib/cn";
import { formatPercent } from "../../../lib/format";
import type { AchievementRow } from "../../../types/domain";

interface DialogRowProps {
  row: AchievementRow;
  rare?: boolean;
  locked?: boolean;
  aside?: ReactNode;
}

export function DialogRow({ row, rare = false, locked = false, aside }: DialogRowProps) {
  const { t, locale } = useI18n();
  return (
    <div className="flex items-center gap-3.5 rounded-card border border-line bg-surface px-3.5 py-2.5">
      <AchievementIcon
        src={locked ? (row.iconGrayUrl ?? row.iconUrl) : row.iconUrl}
        gray={locked}
        rare={rare}
        size={40}
      />
      <div className="mr-auto flex min-w-0 flex-col">
        <span className={cn("font-[650] text-[14px]", locked && "text-muted")}>
          {row.displayName}
        </span>
        <span className="text-[12.5px] text-muted">{row.description}</span>
        {row.percent !== null && (
          <span className="text-[12px] text-dim">
            {t("achievement.playersOf", { pct: formatPercent(row.percent, locale) })}
          </span>
        )}
      </div>
      {aside}
    </div>
  );
}
