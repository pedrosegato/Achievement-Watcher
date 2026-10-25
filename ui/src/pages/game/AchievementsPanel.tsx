import { memo } from "react";
import { AchievementIcon } from "../../components/AchievementIcon";
import { Button } from "../../components/ui/button";
import { useI18n } from "../../i18n/useI18n";
import { completionPercent, isRare, splitLocked } from "../../lib/achievements";
import { formatDateTime } from "../../lib/format";
import { useAppSettings, useRareThreshold } from "../../settings/useAppSettings";
import type { AchievementFilter, GameDetail } from "../../types/domain";
import { IconStrip } from "./IconStrip";

interface AchievementsPanelProps {
  detail: GameDetail;
  onOpenDialog: (filter: AchievementFilter) => void;
}

export const AchievementsPanel = memo(function AchievementsPanel({
  detail,
  onOpenDialog,
}: AchievementsPanelProps) {
  const { t, locale } = useI18n();
  const threshold = useRareThreshold();
  const maskHidden = useAppSettings()?.hideHiddenAchievements ?? true;
  const split = splitLocked(detail.achievements, maskHidden);
  const latest = split.unlocked[0];
  const pct = completionPercent(detail.unlocked, detail.total);

  return (
    <aside className="game-panel flex flex-col gap-2">
      <h2 className="font-semibold text-[12.5px] text-muted">{t("game.panel.title")}</h2>
      <div className="flex flex-col gap-3 rounded-card border border-line bg-surface p-3.5">
        <div className="flex items-baseline gap-1.5 text-[13px] text-muted">
          <span>
            {t("game.panel.progress", { unlocked: detail.unlocked, total: detail.total })}
          </span>
          {detail.total > 0 && (
            <strong className="ml-auto font-bold text-[20px] text-text tracking-[-0.02em]">
              {pct}%
            </strong>
          )}
        </div>
        {latest && latest.unlockedAt !== null && (
          <div className="flex items-center gap-3 border-line border-b pb-3">
            <AchievementIcon
              src={latest.iconUrl}
              size={46}
              rare={isRare(latest.percent, threshold)}
            />
            <div className="flex min-w-0 flex-col">
              <span className="text-[11.5px] text-dim">{t("game.panel.latest")}</span>
              <span className="font-[650] text-[14px]">{latest.displayName}</span>
              <span className="text-[12.5px] text-muted">
                {formatDateTime(latest.unlockedAt, locale)}
              </span>
            </div>
          </div>
        )}
        {split.unlocked.length > 0 && (
          <>
            <span className="-mb-1 text-[11.5px] text-dim">{t("game.panel.unlocked")}</span>
            <IconStrip
              rows={split.unlocked}
              moreLabel={t("game.panel.moreUnlocked")}
              onMore={() => onOpenDialog("unlocked")}
            />
          </>
        )}
        {split.locked.length > 0 && (
          <>
            <span className="-mb-1 text-[11.5px] text-dim">{t("game.panel.locked")}</span>
            <IconStrip
              rows={split.locked}
              gray
              moreLabel={t("game.panel.moreLocked")}
              onMore={() => onOpenDialog("locked")}
            />
          </>
        )}
        {detail.achievements.length > 0 && (
          <Button onClick={() => onOpenDialog("all")} className="w-full bg-surface-2">
            {t("game.panel.all")}
          </Button>
        )}
      </div>
    </aside>
  );
});
