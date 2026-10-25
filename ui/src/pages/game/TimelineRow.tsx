import { AchievementIcon } from "../../components/AchievementIcon";
import { useI18n } from "../../i18n/useI18n";
import { isRare } from "../../lib/achievements";
import { formatPercent, formatTime } from "../../lib/format";
import { useRareThreshold } from "../../settings/useAppSettings";
import type { AchievementRow } from "../../types/domain";

interface TimelineRowProps {
  row: AchievementRow;
}

export function TimelineRow({ row }: TimelineRowProps) {
  const { t, locale } = useI18n();
  const threshold = useRareThreshold();
  return (
    <div className="flex items-center gap-3.5 px-3.5 py-[11px]">
      <AchievementIcon src={row.iconUrl} size={40} rare={isRare(row.percent, threshold)} />
      <div className="mr-auto flex min-w-0 flex-col">
        <span className="font-[650] text-[14px]">{row.displayName}</span>
        <span className="text-[12.5px] text-muted">{row.description}</span>
        {row.percent !== null && (
          <span className="text-[12px] text-dim">
            {t("achievement.playersOf", { pct: formatPercent(row.percent, locale) })}
          </span>
        )}
      </div>
      {row.unlockedAt !== null && (
        <time className="whitespace-nowrap text-[12.5px] text-muted">
          {formatTime(row.unlockedAt, locale)}
        </time>
      )}
    </div>
  );
}
