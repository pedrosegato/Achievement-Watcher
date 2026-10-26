import { AchievementHoverCard } from "../../components/AchievementHoverCard";
import { AchievementIcon } from "../../components/AchievementIcon";
import { isRare } from "../../lib/achievements";
import { useRareThreshold } from "../../settings/useAppSettings";
import type { AchievementRow } from "../../types/domain";

interface IconStripProps {
  rows: AchievementRow[];
  gray?: boolean;
  moreLabel: string;
  onMore: () => void;
}

const SHOWN = 7;

export function IconStrip({ rows, gray = false, moreLabel, onMore }: IconStripProps) {
  const threshold = useRareThreshold();
  const extra = rows.length - SHOWN;
  return (
    <div className="flex items-center gap-[7px]">
      {rows.slice(0, SHOWN).map((row) => {
        const rare = !gray && isRare(row.percent, threshold);
        return (
          <AchievementHoverCard key={row.apiName} achievement={row} rare={rare}>
            <button type="button" aria-label={row.displayName} className="block cursor-default">
              <AchievementIcon
                src={gray ? (row.iconGrayUrl ?? row.iconUrl) : row.iconUrl}
                gray={gray}
                rare={rare}
                size={30}
              />
            </button>
          </AchievementHoverCard>
        );
      })}
      {extra > 0 && (
        <button
          type="button"
          aria-label={moreLabel}
          onClick={onMore}
          className="grid size-[30px] cursor-pointer place-items-center bg-surface-2 font-bold text-[11.5px] text-muted transition-colors duration-(--dur-fast) ease-(--ease-out) hover:bg-line hover:text-text"
        >
          +{extra}
        </button>
      )}
    </div>
  );
}
