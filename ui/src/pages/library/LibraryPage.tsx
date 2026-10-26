import { useQuery } from "@tanstack/react-query";
import { memo, useMemo, useState } from "react";
import { ErrorAlert } from "../../components/ErrorAlert";
import { Icon } from "../../components/Icon";
import { Segmented } from "../../components/Segmented";
import { Button } from "../../components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../../components/ui/select";
import { Tooltip } from "../../components/ui/tooltip";
import { useStoredChoice } from "../../hooks/useStoredChoice";
import type { Key } from "../../i18n/dictionary";
import { useI18n } from "../../i18n/useI18n";
import { cn } from "../../lib/cn";
import { errorMessage } from "../../lib/commands";
import { queries } from "../../lib/queries";
import { GameCard } from "./GameCard";
import { LibraryEmpty } from "./LibraryEmpty";
import { LibrarySkeleton } from "./LibrarySkeleton";
import { GAME_SORTS, type GameSort, sortGames } from "./sort";
import {
  hasMixedOrigins,
  inScope,
  LIBRARY_SCOPES,
  type LibraryScope,
  scopeCounts,
} from "./sources";

interface LibraryPageProps {
  query: string;
  onSearch: (query: string) => void;
  scanning: number | null;
  onRescan: () => void;
  onOpen: (appId: number) => void;
}

const SORT_LABELS: Record<GameSort, Key> = {
  recent: "library.sort.recent",
  name: "library.sort.name",
  progress: "library.sort.progress",
  playtime: "library.sort.playtime",
};

const SCOPE_KEY = "library.scope";

const SCOPE_LABELS: Record<LibraryScope, Key> = {
  all: "library.scope.all",
  steam: "library.scope.steam",
  emulators: "library.scope.emulators",
};

const isSort = (value: string): value is GameSort =>
  (GAME_SORTS as readonly string[]).includes(value);

export const LibraryPage = memo(function LibraryPage({
  query,
  onSearch,
  scanning,
  onRescan,
  onOpen,
}: LibraryPageProps) {
  const { t } = useI18n();
  const busy = scanning !== null;
  const rescanLabel = busy ? t("library.scanning", { pct: scanning }) : t("library.rescan");
  const { data: games, error } = useQuery(queries.games());
  const [sort, setSort] = useState<GameSort>("recent");
  const [scope, setScope] = useStoredChoice(SCOPE_KEY, LIBRARY_SCOPES, "all");
  const counts = useMemo(() => scopeCounts(games ?? []), [games]);
  const mixed = hasMixedOrigins(games ?? []);
  const activeScope = mixed ? scope : "all";
  const scopes = LIBRARY_SCOPES.map((value) => ({
    value,
    label: t(SCOPE_LABELS[value]),
    count: counts[value],
  }));

  const ordered = useMemo(() => sortGames(games ?? [], sort), [games, sort]);
  const shown = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return new Set(
      (games ?? [])
        .filter(
          (game) =>
            inScope(game, activeScope) &&
            (!needle ||
              game.name.toLocaleLowerCase().includes(needle) ||
              String(game.appId) === needle),
        )
        .map((game) => game.appId),
    );
  }, [games, query, activeScope]);

  let content = <LibrarySkeleton />;
  if (games?.length === 0) content = <LibraryEmpty onRescan={onRescan} scanning={scanning} />;
  else if (games && shown.size === 0) {
    content = <p className="py-10 text-center text-[13px] text-dim">{t("library.noMatch")}</p>;
  } else if (games) {
    content = (
      <div className="library-grid">
        {ordered.map((game) => (
          <div key={game.appId} hidden={!shown.has(game.appId)} className="grid">
            <GameCard game={game} onOpen={onOpen} />
          </div>
        ))}
      </div>
    );
  }

  return (
    <section className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <div className="mr-auto flex items-center gap-6">
          <h1 className="font-bold text-[22px] tracking-[-0.01em]">{t("library.title")}</h1>
          {games && mixed && (
            <Segmented
              variant="tabs"
              label={t("library.scope.label")}
              options={scopes}
              value={activeScope}
              onChange={setScope}
            />
          )}
          {games && !mixed && <span className="text-[14px] text-dim">{shown.size}</span>}
        </div>
        <label className="flex h-8 w-[240px] items-center gap-2 rounded-card border border-line bg-surface px-2.5 text-[13px] text-dim focus-within:text-muted">
          <Icon name="search" size={16} />
          <input
            type="search"
            value={query}
            onChange={(event) => onSearch(event.target.value)}
            placeholder={t("library.search")}
            aria-label={t("library.search")}
            className="w-full bg-transparent text-text outline-none placeholder:text-dim"
          />
        </label>
        <Select value={sort} onValueChange={(value) => isSort(value) && setSort(value)}>
          <SelectTrigger aria-label={t("library.sort.label")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent align="end">
            {GAME_SORTS.map((option) => (
              <SelectItem key={option} value={option}>
                {t(SORT_LABELS[option])}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Tooltip label={rescanLabel}>
          <Button
            size="icon"
            aria-label={rescanLabel}
            aria-busy={busy}
            onClick={busy ? undefined : onRescan}
            className="text-muted hover:text-text"
          >
            <Icon name="sync" size={16} className={cn(busy && "spinning")} />
          </Button>
        </Tooltip>
      </div>
      <ErrorAlert message={error ? errorMessage(error) : null} />
      {content}
    </section>
  );
});
