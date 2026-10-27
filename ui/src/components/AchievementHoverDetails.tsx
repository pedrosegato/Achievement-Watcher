import { useI18n } from "../i18n/useI18n";
import { formatDateTime, formatHours, formatPercent } from "../lib/format";
import type { AchievementRow, RecentIcon } from "../types/domain";
import { AchievementIcon } from "./AchievementIcon";

interface Column {
  label: string;
  value: string;
}

export function AchievementHoverDetails({
  achievement,
  rare,
}: {
  achievement: RecentIcon | AchievementRow;
  rare: boolean;
}) {
  const { t, locale } = useI18n();
  const columns: Column[] = [];
  if (achievement.unlockedAt !== null) {
    columns.push({
      label: t("achievement.unlockedOn"),
      value: formatDateTime(achievement.unlockedAt, locale),
    });
  }
  if (achievement.playtimeSeconds) {
    columns.push({
      label: t("achievement.playtime"),
      value: formatHours(achievement.playtimeSeconds, locale),
    });
  }
  if (achievement.percent !== null) {
    columns.push({
      label: t("achievement.players"),
      value: formatPercent(achievement.percent, locale),
    });
  }

  return (
    <>
      <div className="flex items-start gap-2.5">
        <AchievementIcon src={achievement.iconUrl} rare={rare} size={40} />
        <div className="flex min-w-0 flex-col gap-px">
          <p className="line-clamp-2 font-semibold text-[13px] leading-snug">
            {achievement.displayName}
          </p>
          <p className="line-clamp-2 text-[12px] text-muted leading-snug">
            {achievement.description}
          </p>
        </div>
      </div>
      {columns.length > 0 && (
        <dl className="flex justify-between gap-3 border-line border-t pt-2.5">
          {columns.map((column) => (
            <div key={column.label} className="flex min-w-0 flex-col gap-px">
              <dt className="text-[10.5px] text-dim">{column.label}</dt>
              <dd className="whitespace-nowrap text-[12px]">{column.value}</dd>
            </div>
          ))}
        </dl>
      )}
    </>
  );
}
