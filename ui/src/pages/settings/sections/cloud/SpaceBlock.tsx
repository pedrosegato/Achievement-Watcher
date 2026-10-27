import { Icon } from "../../../../components/Icon";
import { Button } from "../../../../components/ui/button";
import { useCloudAction } from "../../../../hooks/useCloudAction";
import { useI18n } from "../../../../i18n/useI18n";
import { api } from "../../../../lib/commands";
import { formatBytes } from "../../../../lib/format";
import type { CloudOverview } from "../../../../types/domain";

export function SpaceBlock({ overview }: { overview: CloudOverview }) {
  const { t, locale } = useI18n();
  const { run } = useCloudAction();
  const folder = overview.folder ?? "";

  return (
    <div className="flex flex-col gap-3 py-3.5">
      <div className="flex items-baseline justify-between gap-4">
        <span className="font-semibold text-[14px]">{t("cloud.space")}</span>
        <span className="font-semibold text-[14px]">
          {formatBytes(overview.totalBytes, locale)}
        </span>
      </div>
      {overview.games.length > 0 ? (
        <ul className="flex flex-col rounded-card border border-line bg-surface">
          {overview.games.map((game) => (
            <li
              key={game.key}
              className="flex items-center justify-between gap-4 border-line border-b px-3.5 py-2.5 text-[13px] last:border-b-0"
            >
              <span className="truncate">{game.name}</span>
              <span className="flex-none text-dim">{formatBytes(game.bytes, locale)}</span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="rounded-card border border-line border-dashed p-3 text-[13px] text-dim">
          {t("cloud.noGames")}
        </p>
      )}
      <div className="flex items-center justify-between gap-4">
        <p className="text-[12.5px] text-dim">{t("cloud.cleanup")}</p>
        <Button
          size="sm"
          className="flex-none"
          onClick={() => void run(() => api.openPath(folder))}
        >
          <Icon name="folder-open" size={16} />
          {t("cloud.openFolder")}
        </Button>
      </div>
    </div>
  );
}
