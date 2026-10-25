import type { GameRow } from "../../types/domain";

export const GAME_SORTS = ["recent", "name", "progress", "playtime"] as const;
export type GameSort = (typeof GAME_SORTS)[number];

const byName = (a: GameRow, b: GameRow) =>
  a.name.localeCompare(b.name, undefined, { sensitivity: "base" });

const progressOf = (row: GameRow) => (row.total > 0 ? row.unlocked / row.total : -1);

const COMPARE: Record<GameSort, (a: GameRow, b: GameRow) => number> = {
  recent: (a, b) => (b.lastUnlockAt ?? -1) - (a.lastUnlockAt ?? -1),
  name: () => 0,
  progress: (a, b) => progressOf(b) - progressOf(a),
  playtime: (a, b) => b.playtimeSeconds - a.playtimeSeconds,
};

export function sortGames(rows: GameRow[], sort: GameSort): GameRow[] {
  const compare = COMPARE[sort];
  return [...rows].sort((a, b) => compare(a, b) || byName(a, b));
}
