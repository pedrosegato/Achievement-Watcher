import { describe, expect, it } from "vitest";
import type { GameRow } from "../../types/domain";
import { hasMixedOrigins, inScope, scopeCounts } from "./sources";

function game(appId: number, originId: string | null): GameRow {
  return {
    appId,
    name: `Game ${appId}`,
    unlocked: 0,
    total: 0,
    lastUnlockAt: null,
    coverUrl: "",
    playtimeSeconds: 0,
    lastPlayed: null,
    playing: false,
    origin: originId,
    originId,
    headerUrl: "",
    platinumAt: null,
    recent: [],
  };
}

const games = [game(1, "steam"), game(2, "onlinefix"), game(3, "steam"), game(4, null)];
const ids = (scope: Parameters<typeof inScope>[1]) =>
  games.filter((row) => inScope(row, scope)).map((row) => row.appId);

describe("inScope", () => {
  it("shows every game in all", () => {
    expect(ids("all")).toEqual([1, 2, 3, 4]);
  });

  it("shows only Steam games in steam", () => {
    expect(ids("steam")).toEqual([1, 3]);
  });

  it("shows everything outside Steam in emulators", () => {
    expect(ids("emulators")).toEqual([2, 4]);
  });
});

describe("scopeCounts", () => {
  it("counts each scope", () => {
    expect(scopeCounts(games)).toEqual({ all: 4, steam: 2, emulators: 2 });
  });
});

describe("hasMixedOrigins", () => {
  it("offers the tabs only when Steam and non-Steam games are both there", () => {
    expect(hasMixedOrigins(games)).toBe(true);
    expect(hasMixedOrigins([game(1, "steam")])).toBe(false);
    expect(hasMixedOrigins([game(2, "onlinefix")])).toBe(false);
  });
});
