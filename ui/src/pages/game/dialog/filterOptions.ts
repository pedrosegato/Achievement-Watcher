import type { SegmentedOption } from "../../../components/Segmented";
import { ACHIEVEMENT_FILTERS } from "../../../constants";
import { FILTER_LABELS } from "../../../i18n/labels";
import { isUnlocked } from "../../../lib/achievements";
import type { AchievementFilter, AchievementRow } from "../../../types/domain";
import type { Translate } from "../../../types/ui";

export function filterOptions(
  rows: AchievementRow[],
  t: Translate,
): SegmentedOption<AchievementFilter>[] {
  const unlocked = rows.filter(isUnlocked).length;
  const counts: Record<AchievementFilter, number> = {
    all: rows.length,
    unlocked,
    locked: rows.length - unlocked,
  };
  return ACHIEVEMENT_FILTERS.map((value) => ({
    value,
    label: t(FILTER_LABELS[value]),
    count: counts[value],
  }));
}
