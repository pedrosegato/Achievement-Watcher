import type { AchievementFilter, AchievementRow } from "../types/domain";

const PERCENT = 100;

interface DayGroup {
  key: string;
  day: number;
  items: AchievementRow[];
}

interface LimitedGroup extends DayGroup {
  total: number;
}

interface LockedSplit {
  unlocked: AchievementRow[];
  locked: AchievementRow[];
  hidden: AchievementRow[];
}

export const isUnlocked = (row: AchievementRow) => row.unlockedAt !== null;

function isMasked(row: AchievementRow, hideHidden: boolean): boolean {
  return hideHidden && row.hidden && !isUnlocked(row);
}

function filterAchievements(rows: AchievementRow[], filter: AchievementFilter): AchievementRow[] {
  if (filter === "unlocked") return rows.filter(isUnlocked);
  if (filter === "locked") return rows.filter((row) => !isUnlocked(row));
  return rows;
}

const byNewest = (a: AchievementRow, b: AchievementRow) =>
  (b.unlockedAt ?? 0) - (a.unlockedAt ?? 0);

function dayKey(seconds: number): string {
  const date = new Date(seconds * 1000);
  return `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`;
}

export function groupByDay(rows: AchievementRow[]): DayGroup[] {
  const groups = new Map<string, DayGroup>();
  for (const row of rows.filter(isUnlocked).sort(byNewest)) {
    const at = row.unlockedAt ?? 0;
    const key = dayKey(at);
    const group = groups.get(key);
    if (group) group.items.push(row);
    else groups.set(key, { key, day: at, items: [row] });
  }
  return [...groups.values()];
}

export function limitRows(groups: DayGroup[], budget: number): LimitedGroup[] {
  const limited: LimitedGroup[] = [];
  let left = budget;
  for (const group of groups) {
    if (left <= 0) break;
    const items = left >= group.items.length ? group.items : group.items.slice(0, left);
    limited.push({ ...group, items, total: group.items.length });
    left -= items.length;
  }
  return limited;
}

export function limitSplit(split: LockedSplit, budget: number): LockedSplit {
  const unlocked = split.unlocked.slice(0, budget);
  const locked = split.locked.slice(0, budget - unlocked.length);
  const hidden = split.hidden.slice(0, budget - unlocked.length - locked.length);
  return { unlocked, locked, hidden };
}

export function isRare(percent: number | null, threshold: number): boolean {
  return percent !== null && percent < threshold;
}

export function rarest(rows: AchievementRow[]): AchievementRow | null {
  let best: AchievementRow | null = null;
  for (const row of rows) {
    if (!isUnlocked(row) || row.percent === null) continue;
    if (best === null || row.percent < (best.percent ?? Number.POSITIVE_INFINITY)) best = row;
  }
  return best;
}

export function splitLocked(rows: AchievementRow[], maskHidden: boolean): LockedSplit {
  return {
    unlocked: rows.filter(isUnlocked).sort(byNewest),
    locked: rows.filter((row) => !isUnlocked(row) && !isMasked(row, maskHidden)),
    hidden: rows.filter((row) => isMasked(row, maskHidden)),
  };
}

export function filterDialog(
  rows: AchievementRow[],
  filter: AchievementFilter,
  query: string,
  maskHidden = true,
): AchievementRow[] {
  const needle = query.trim().toLocaleLowerCase();
  return filterAchievements(rows, filter).filter((row) => {
    if (!needle) return true;
    if (isMasked(row, maskHidden)) return false;
    return (
      row.displayName.toLocaleLowerCase().includes(needle) ||
      row.description.toLocaleLowerCase().includes(needle)
    );
  });
}

export function completionPercent(unlocked: number, total: number): number {
  return total > 0 ? Math.floor((unlocked * PERCENT) / total) : 0;
}
