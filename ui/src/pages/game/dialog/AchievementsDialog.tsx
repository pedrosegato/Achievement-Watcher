import { useReducer, useRef, useState } from "react";
import { ArtImage } from "../../../components/ArtImage";
import { Icon } from "../../../components/Icon";
import { Segmented } from "../../../components/Segmented";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "../../../components/ui/dialog";
import { useI18n } from "../../../i18n/useI18n";
import { completionPercent } from "../../../lib/achievements";
import { useAppSettings } from "../../../settings/useAppSettings";
import type { AchievementFilter, GameDetail } from "../../../types/domain";
import { AchievementList } from "./AchievementList";
import { filterOptions } from "./filterOptions";
import { hiddenReducer } from "./hidden";

interface AchievementsDialogProps {
  detail: GameDetail;
  open: boolean;
  filter: AchievementFilter;
  onFilter: (filter: AchievementFilter) => void;
  onOpenChange: (open: boolean) => void;
}

const NONE: ReadonlySet<string> = new Set();

export function AchievementsDialog({
  detail,
  open,
  filter,
  onFilter,
  onOpenChange,
}: AchievementsDialogProps) {
  const { t } = useI18n();
  const maskHidden = useAppSettings()?.hideHiddenAchievements ?? true;
  const [query, setQuery] = useState("");
  const [revealed, dispatch] = useReducer(hiddenReducer, NONE);
  const search = useRef<HTMLInputElement>(null);

  const pct = completionPercent(detail.unlocked, detail.total);

  const change = (next: boolean) => {
    if (!next) {
      dispatch({ type: "reset" });
      setQuery("");
    }
    onOpenChange(next);
  };

  const options = filterOptions(detail.achievements, t);

  return (
    <Dialog open={open} onOpenChange={change}>
      <DialogContent
        className="max-h-[min(82vh,780px)] w-[min(680px,calc(100vw-32px))] bg-bg"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          search.current?.focus();
        }}
      >
        <header className="relative mb-3.5 overflow-hidden bg-[#111] px-[22px] pt-[26px] pb-[18px]">
          <div className="absolute inset-0">
            <ArtImage src={detail.heroUrl} />
          </div>
          <div className="absolute inset-0 bg-[rgba(8,9,11,0.62)]" />
          <div className="relative flex flex-col gap-1.5 text-white">
            <DialogTitle className="pr-10 font-bold text-[22px]">{detail.name}</DialogTitle>
            <DialogDescription className="flex items-baseline gap-2.5 text-[13px] text-white/75">
              <span>
                {t("dialog.progress", { unlocked: detail.unlocked, total: detail.total })}
              </span>
              {detail.total > 0 && <b className="ml-auto text-[18px] text-white">{pct}%</b>}
            </DialogDescription>
          </div>
          <DialogClose asChild>
            <button
              type="button"
              aria-label={t("window.close")}
              className="absolute top-3 right-3 grid size-8 cursor-pointer place-items-center rounded-card border border-white/15 bg-[rgba(8,9,11,0.5)] text-white"
            >
              <Icon name="close" size={16} />
            </button>
          </DialogClose>
        </header>
        <div className="flex flex-wrap gap-2 px-[22px] pb-3">
          <label className="flex min-w-[200px] flex-1 items-center gap-2 rounded-card border border-line bg-surface px-3 py-[7px] text-[13px] text-dim focus-within:text-muted">
            <Icon name="search" size={16} />
            <input
              ref={search}
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("dialog.search")}
              aria-label={t("dialog.search")}
              className="w-full bg-transparent text-text outline-none placeholder:text-dim"
            />
          </label>
          <Segmented
            options={options}
            value={filter}
            onChange={onFilter}
            label={t("dialog.filter")}
          />
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto px-[22px] pb-[22px]">
          <AchievementList
            rows={detail.achievements}
            filter={filter}
            query={query}
            maskHidden={maskHidden}
            revealed={revealed}
            dispatch={dispatch}
          />
        </div>
      </DialogContent>
    </Dialog>
  );
}
