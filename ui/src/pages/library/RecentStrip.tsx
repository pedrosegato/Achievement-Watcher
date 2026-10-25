import { AchievementHoverCard } from "../../components/AchievementHoverCard";
import { AchievementIcon } from "../../components/AchievementIcon";
import { useI18n } from "../../i18n/useI18n";
import { isRare } from "../../lib/achievements";
import { formatDay } from "../../lib/format";
import { useRareThreshold } from "../../settings/useAppSettings";
import type { GameRow } from "../../types/domain";

interface RecentStripProps {
  game: GameRow;
  onOpen: () => void;
}

export function RecentStrip({ game, onOpen }: RecentStripProps) {
  const { t, locale } = useI18n();
  const threshold = useRareThreshold();

  if (game.recent.length === 0) {
    return <span className="text-[12.5px] text-dim">{t("library.noneYet")}</span>;
  }

  const label =
    game.platinumAt !== null
      ? t("library.platinumOn", { date: formatDay(game.platinumAt, locale) })
      : t("library.latest");

  return (
    <>
      <span className="text-[11.5px] text-dim">{label}</span>
      <div className="flex items-center gap-[7px]">
        {game.recent.map((item) => {
          const rare = isRare(item.percent, threshold);
          return (
            <AchievementHoverCard key={item.apiName} achievement={item} rare={rare}>
              <button
                type="button"
                aria-label={item.displayName}
                onClick={onOpen}
                className="relative z-1 block cursor-pointer"
              >
                <AchievementIcon src={item.iconUrl} size={30} rare={rare} />
              </button>
            </AchievementHoverCard>
          );
        })}
      </div>
    </>
  );
}
