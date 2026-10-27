import type { Key } from "../../../i18n/dictionary";
import { useI18n } from "../../../i18n/useI18n";
import type { PlayMoment, PlayPhase } from "../../../lib/play";
import { GamepadGlyph } from "./GamepadGlyph";
import { PlayGlyph } from "./PlayGlyph";
import { SyncedBurst } from "./SyncedBurst";

interface PlayContentProps {
  phase: PlayPhase;
  labelKey: Key;
  moment: PlayMoment | null;
  relabelled: boolean;
}

export function PlayContent({ phase, labelKey, moment, relabelled }: PlayContentProps) {
  const { t } = useI18n();

  if (phase === "launching") {
    return (
      <>
        <GamepadGlyph />
        <span className="sr-only">{t(labelKey)}</span>
      </>
    );
  }
  if (phase === "closing") {
    return (
      <>
        <GamepadGlyph off />
        <span className={relabelled ? "label-swap" : undefined}>{t(labelKey)}</span>
      </>
    );
  }
  if (moment === "synced") {
    return (
      <>
        <SyncedBurst />
        <span className="label-swap">{t("cloud.status.synced")}</span>
      </>
    );
  }
  return (
    <>
      <PlayGlyph key={phase} phase={phase} />
      <span key={labelKey} className={relabelled ? "label-swap" : undefined}>
        {t(labelKey)}
      </span>
    </>
  );
}
