import type { GameRow } from "../../types/domain";

export const LIBRARY_SCOPES = ["all", "steam", "emulators"] as const;
export type LibraryScope = (typeof LIBRARY_SCOPES)[number];

const STEAM = "steam";

export function inScope(game: GameRow, scope: LibraryScope): boolean {
  if (scope === "steam") return game.originId === STEAM;
  if (scope === "emulators") return game.originId !== STEAM;
  return true;
}

export function scopeCounts(games: GameRow[]): Record<LibraryScope, number> {
  const steam = games.filter((game) => game.originId === STEAM).length;
  return { all: games.length, steam, emulators: games.length - steam };
}

export function hasMixedOrigins(games: GameRow[]): boolean {
  const { steam, emulators } = scopeCounts(games);
  return steam > 0 && emulators > 0;
}
