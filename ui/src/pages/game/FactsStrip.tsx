import { AchievementIcon } from "../../components/AchievementIcon";
import { Fact } from "../../components/Fact";
import { useI18n } from "../../i18n/useI18n";
import { completionPercent, rarest } from "../../lib/achievements";
import { formatDateTime, formatHours, formatPercent } from "../../lib/format";
import type { GameDetail } from "../../types/domain";
import { CloudFact } from "./cloud/CloudFact";
import { PlayButton } from "./play/PlayButton";

interface FactsStripProps {
  detail: GameDetail;
  onOpenSettings?: () => void;
}

export function FactsStrip({ detail, onOpenSettings }: FactsStripProps) {
  const { t, locale } = useI18n();
  const best = rarest(detail.achievements);
  const last = detail.achievements.reduce<number | null>(
    (latest, row) =>
      row.unlockedAt !== null && row.unlockedAt > (latest ?? 0) ? row.unlockedAt : latest,
    null,
  );
  const pct = completionPercent(detail.unlocked, detail.total);

  return (
    <div className="flex flex-wrap items-center gap-x-[34px] gap-y-3 border-line border-b px-5 py-4">
      {onOpenSettings && <PlayButton appId={detail.appId} />}
      {detail.origin && <Fact label={t("game.facts.origin")} value={detail.origin} />}
      <Fact
        icon="trophy"
        label={t("game.facts.achievements")}
        value={
          detail.total > 0
            ? t("game.facts.progress", { unlocked: detail.unlocked, total: detail.total, pct })
            : t("game.facts.unlockedOnly", { unlocked: detail.unlocked })
        }
      />
      {detail.playtimeSeconds > 0 && (
        <Fact
          icon="clock"
          label={t("game.facts.playtime")}
          value={formatHours(detail.playtimeSeconds, locale)}
        />
      )}
      {last !== null && (
        <Fact icon="calendar" label={t("game.facts.last")} value={formatDateTime(last, locale)} />
      )}
      {best && best.percent !== null && (
        <Fact
          lead={<AchievementIcon src={best.iconUrl} size={28} rare />}
          label={t("game.facts.rarest")}
          value={
            <>
              {best.displayName}{" "}
              <span className="font-normal text-muted">{formatPercent(best.percent, locale)}</span>
            </>
          }
        />
      )}
      {onOpenSettings && (
        <CloudFact appId={detail.appId} gameName={detail.name} onOpenSettings={onOpenSettings} />
      )}
    </div>
  );
}
