import { describe, expect, it } from "vitest";
import type { GameRow } from "../../types/domain";
import { sortGames } from "./sort";

function game(name: string, partial: Partial<GameRow> = {}): GameRow {
  return {
    appId: name.length,
    name,
    unlocked: 0,
    total: 0,
    lastUnlockAt: null,
    coverUrl: "",
    playtimeSeconds: 0,
    lastPlayed: null,
    playing: false,
    origin: null,
    originId: null,
    headerUrl: "",
    platinumAt: null,
    recent: [],
    ...partial,
  };
}

const rows = [
  game("Zeta"),
  game("beta", { lastUnlockAt: 50, unlocked: 5, total: 10, playtimeSeconds: 100 }),
  game("Alpha", { lastUnlockAt: 50, unlocked: 1, total: 2, playtimeSeconds: 900 }),
  game("Gamma", { lastUnlockAt: 90, unlocked: 9, total: 10 }),
  game("Delta"),
];

const names = (list: GameRow[]) => list.map((row) => row.name);

describe("sortGames", () => {
  it("puts the latest unlock first, ties by name and games without unlocks last", () => {
    expect(names(sortGames(rows, "recent"))).toEqual(["Gamma", "Alpha", "beta", "Delta", "Zeta"]);
  });

  it("sorts by name ignoring case", () => {
    expect(names(sortGames(rows, "name"))).toEqual(["Alpha", "beta", "Delta", "Gamma", "Zeta"]);
  });

  it("sorts by progress with unknown totals last", () => {
    expect(names(sortGames(rows, "progress"))).toEqual(["Gamma", "Alpha", "beta", "Delta", "Zeta"]);
  });

  it("sorts by playtime", () => {
    expect(names(sortGames(rows, "playtime"))).toEqual(["Alpha", "beta", "Delta", "Gamma", "Zeta"]);
  });

  it("does not mutate the input", () => {
    sortGames(rows, "name");
    expect(rows[0]?.name).toBe("Zeta");
  });
});
