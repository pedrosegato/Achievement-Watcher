import { useState } from "react";
import { AchievementIcon } from "../../components/AchievementIcon";
import { Collapsible } from "../../components/Collapsible";
import { useI18n } from "../../i18n/useI18n";
import { groupByDay, isRare } from "../../lib/achievements";
import { formatDay } from "../../lib/format";
import { useRareThreshold } from "../../settings/useAppSettings";
import type { GameDetail } from "../../types/domain";
import { TimelineRow } from "./TimelineRow";

interface ActivityTimelineProps {
  detail: GameDetail;
}

const PEEK = 8;

export function ActivityTimeline({ detail }: ActivityTimelineProps) {
  const { t, locale } = useI18n();
  const threshold = useRareThreshold();
  const [closed, setClosed] = useState<ReadonlySet<string>>(() => new Set());
  const groups = groupByDay(detail.achievements);
  const toggle = (key: string) =>
    setClosed((current) => {
      const next = new Set(current);
      if (!next.delete(key)) next.add(key);
      return next;
    });

  return (
    <section className="flex min-w-0 flex-col gap-2">
      <h2 className="font-semibold text-[12.5px] text-muted">{t("game.activity")}</h2>
      {groups.length === 0 && <p className="text-[13px] text-dim">{t("library.noneYet")}</p>}
      {groups.map((group) => (
        <Collapsible
          key={group.key}
          card
          open={!closed.has(group.key)}
          onToggle={() => toggle(group.key)}
          header={formatDay(group.day, locale)}
          count={
            group.items.length === 1
              ? t("game.dayCountOne")
              : t("game.dayCountOther", { n: group.items.length })
          }
          peek={group.items
            .slice(0, PEEK)
            .map((row) => (
              <AchievementIcon
                key={row.apiName}
                src={row.iconUrl}
                size={20}
                rare={isRare(row.percent, threshold)}
              />
            ))}
        >
          {group.items.map((row) => (
            <TimelineRow key={row.apiName} row={row} />
          ))}
        </Collapsible>
      ))}
    </section>
  );
}
