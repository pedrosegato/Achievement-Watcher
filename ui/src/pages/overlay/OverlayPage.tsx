import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useReducer, useState } from "react";
import { Segmented } from "../../components/Segmented";
import { DEFAULT_HOTKEY, EVENTS, OVERLAY_CLASS } from "../../constants";
import { useTauriEvent } from "../../hooks/useTauriEvent";
import { useI18n } from "../../i18n/useI18n";
import { queries } from "../../lib/queries";
import { useAppSettings } from "../../settings/useAppSettings";
import type { AchievementFilter } from "../../types/domain";
import { AchievementList } from "../game/dialog/AchievementList";
import { filterOptions } from "../game/dialog/filterOptions";
import { hiddenReducer } from "../game/dialog/hidden";
import { FactsStrip } from "../game/FactsStrip";

const NONE: ReadonlySet<string> = new Set();

export function OverlayPage() {
  const { t } = useI18n();
  const settings = useAppSettings();
  const { data: game, refetch } = useQuery(queries.overlayGame());
  const [filter, setFilter] = useState<AchievementFilter>("all");
  const [revealed, dispatch] = useReducer(hiddenReducer, NONE);
  const onShown = useCallback(() => {
    void refetch();
    dispatch({ type: "reset" });
  }, [refetch]);
  useTauriEvent(EVENTS.gamesChanged, () => void refetch());
  useTauriEvent(EVENTS.overlayShown, onShown);

  useEffect(() => {
    document.documentElement.classList.add(OVERLAY_CLASS);
  }, []);

  return (
    <div className="overlay-panel flex h-screen flex-col overflow-hidden rounded-card border border-line text-text">
      {game ? (
        <>
          <h1 className="truncate px-6 pt-4 font-bold text-[18px]">{game.name}</h1>
          <FactsStrip detail={game} />
          <div className="px-6 py-3">
            <Segmented
              options={filterOptions(game.achievements, t)}
              value={filter}
              onChange={setFilter}
              label={t("dialog.filter")}
            />
          </div>
          <div className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto px-6 pb-4">
            <AchievementList
              rows={game.achievements}
              filter={filter}
              maskHidden={settings?.hideHiddenAchievements ?? true}
              revealed={revealed}
              dispatch={dispatch}
            />
          </div>
        </>
      ) : (
        <p className="flex-1 p-6 text-[13px] text-muted">{t("overlay.empty")}</p>
      )}
      <footer className="border-line border-t px-6 py-2.5 text-[12px] text-dim">
        {t("overlay.hint", { key: settings?.overlayHotkey ?? DEFAULT_HOTKEY })}
      </footer>
    </div>
  );
}
