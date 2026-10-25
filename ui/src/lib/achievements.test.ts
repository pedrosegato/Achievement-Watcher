import { describe, expect, it } from "vitest";
import type { AchievementRow } from "../types/domain";
import {
  completionPercent,
  filterDialog,
  groupByDay,
  isRare,
  rarest,
  splitLocked,
} from "./achievements";

const DAY = 86_400;
const BASE = 1775507489;

function row(apiName: string, partial: Partial<AchievementRow> = {}): AchievementRow {
  return {
    apiName,
    displayName: apiName,
    description: "",
    iconUrl: null,
    iconGrayUrl: null,
    hidden: false,
    percent: null,
    unlockedAt: null,
    playtimeSeconds: null,
    ...partial,
  };
}

const timeline = [
  row("A", { unlockedAt: BASE - 3600 }),
  row("B", { unlockedAt: BASE }),
  row("C", { unlockedAt: BASE - DAY }),
  ...[0, 1, 2, 3, 4].map((i) => row(`D${i}`, { unlockedAt: BASE - 3 * DAY + i * 60 })),
  row("Locked"),
];

const dialog = [
  row("Speedrunner", { unlockedAt: BASE, percent: 12 }),
  row("Secret", { hidden: true, unlockedAt: BASE - 60, percent: 3.4 }),
  row("Legend", { hidden: true }),
  row("Ghost", { hidden: true }),
  row("Collector", { description: "Find every legend" }),
];

describe("groupByDay", () => {
  it("groups unlocked rows by local day, newest first", () => {
    const groups = groupByDay(timeline);
    expect(groups.map((group) => group.items.length)).toEqual([2, 1, 5]);
    expect(groups[0]?.items.map((item) => item.apiName)).toEqual(["B", "A"]);
  });
});

describe("splitLocked", () => {
  it("separates masked hidden achievements", () => {
    const split = splitLocked(dialog, true);
    expect(split.hidden.map((item) => item.apiName)).toEqual(["Legend", "Ghost"]);
    expect(split.locked.map((item) => item.apiName)).toEqual(["Collector"]);
    expect(split.unlocked.map((item) => item.apiName)).toEqual(["Speedrunner", "Secret"]);
  });

  it("keeps hidden achievements as locked when masking is off", () => {
    const split = splitLocked(dialog, false);
    expect(split.hidden).toEqual([]);
    expect(split.locked).toHaveLength(3);
  });
});

describe("filterDialog", () => {
  it("never matches a masked hidden achievement by its real name", () => {
    expect(filterDialog(dialog, "locked", "legend").map((item) => item.apiName)).toEqual([
      "Collector",
    ]);
    expect(filterDialog(dialog, "locked", "ghost")).toEqual([]);
  });

  it("matches unlocked hidden achievements and descriptions", () => {
    expect(filterDialog(dialog, "all", "secret").map((item) => item.apiName)).toEqual(["Secret"]);
    expect(filterDialog(dialog, "unlocked", "").map((item) => item.apiName)).toEqual([
      "Speedrunner",
      "Secret",
    ]);
  });

  it("finds hidden names when masking is off", () => {
    expect(filterDialog(dialog, "locked", "ghost", false).map((item) => item.apiName)).toEqual([
      "Ghost",
    ]);
  });
});

describe("rarity", () => {
  it("picks the rarest unlocked achievement", () => {
    expect(rarest(dialog)?.apiName).toBe("Secret");
    expect(rarest([row("x")])).toBeNull();
  });

  it("is rare strictly below the threshold", () => {
    expect(isRare(7.2, 10)).toBe(true);
    expect(isRare(10, 10)).toBe(false);
    expect(isRare(null, 10)).toBe(false);
  });
});

describe("completionPercent", () => {
  it("floors so a game only shows 100% when it is complete", () => {
    expect(completionPercent(14, 49)).toBe(28);
    expect(completionPercent(48, 49)).toBe(97);
    expect(completionPercent(49, 49)).toBe(100);
  });

  it("is zero without a schema", () => {
    expect(completionPercent(5, 0)).toBe(0);
  });
});
