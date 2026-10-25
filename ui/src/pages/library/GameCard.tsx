import { useQueryClient } from "@tanstack/react-query";
import { ArtImage } from "../../components/ArtImage";
import { OriginTag } from "../../components/OriginTag";
import { useI18n } from "../../i18n/useI18n";
import { completionPercent } from "../../lib/achievements";
import { cn } from "../../lib/cn";
import { formatHours } from "../../lib/format";
import { queries } from "../../lib/queries";
import type { GameRow } from "../../types/domain";
import { RecentStrip } from "./RecentStrip";

interface GameCardProps {
  game: GameRow;
  onOpen: (appId: number) => void;
}

export function GameCard({ game, onOpen }: GameCardProps) {
  const { t, locale } = useI18n();
  const queryClient = useQueryClient();
  const open = () => onOpen(game.appId);
  const warm = () => void queryClient.prefetchQuery(queries.game(game.appId));
  const known = game.total > 0;
  const done = known && game.unlocked >= game.total;
  const percent = completionPercent(game.unlocked, game.total);
  let detail = t("library.unlockedCount", { n: game.unlocked });
  if (done) detail = t("library.platinum");
  else if (known) detail = t("library.left", { n: game.total - game.unlocked });

  return (
    <article
      className={cn(
        "game-card relative flex flex-col rounded-card border border-line bg-surface",
        done && "game-card-platinum",
      )}
    >
      <div className="relative aspect-460/215 overflow-hidden rounded-t-[3px] bg-surface-2">
        <ArtImage src={game.headerUrl} />
        {game.origin && (
          <div className="absolute top-2 left-2 flex gap-1">
            <OriginTag origin={game.origin} />
          </div>
        )}
      </div>
      <div className="px-[13px] pt-[11px] pb-[13px]">
        <h2 className="truncate font-[650] text-[14px]">
          <button
            type="button"
            onClick={open}
            onPointerEnter={warm}
            onFocus={warm}
            className="card-open cursor-pointer outline-none after:absolute after:inset-0 after:content-['']"
          >
            {game.name}
          </button>
        </h2>
        <div className="mt-1.5 flex min-h-[30px] items-center gap-2 text-[12.5px] text-muted">
          <strong className="font-bold text-[22px] text-text tracking-[-0.02em]">
            {known ? `${percent}%` : game.unlocked}
          </strong>
          <span>{detail}</span>
          {game.playtimeSeconds > 0 && (
            <span className="ml-auto">{formatHours(game.playtimeSeconds, locale)}</span>
          )}
        </div>
        <div className="mt-[11px] flex flex-col gap-[7px] border-line border-t pt-2.5">
          <RecentStrip game={game} onOpen={open} />
        </div>
      </div>
    </article>
  );
}
