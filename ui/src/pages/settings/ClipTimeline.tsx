import { useState } from "react";
import { Slider } from "../../components/ui/slider";
import { RANGES } from "../../constants";
import { useI18n } from "../../i18n/useI18n";
import { clipSizeMb } from "../../lib/video";
import type { VideoQuality } from "../../types/domain";

interface ClipTimelineProps {
  before: number;
  after: number;
  fps: number;
  quality: VideoQuality;
  onCommit: (before: number, after: number) => void;
}

type Window = [number, number];

const START = -RANGES.videoBefore.max;
const END = RANGES.videoAfter.max;
const UNLOCK_AT = `${(-START / (END - START)) * 100}%`;

function clampWindow([start, end]: number[], fallback: Window): Window {
  const from = Math.min(-RANGES.videoBefore.min, Math.max(START, start ?? fallback[0]));
  const to = Math.max(RANGES.videoAfter.min, Math.min(END, end ?? fallback[1]));
  return [from, to];
}

export function ClipTimeline({ before, after, fps, quality, onCommit }: ClipTimelineProps) {
  const { t } = useI18n();
  const saved: Window = [-before, after];
  const [draft, setDraft] = useState<Window | null>(null);
  const [start, end] = draft ?? saved;

  return (
    <div className="flex flex-col gap-1.5">
      <div className="relative h-4 text-[12px] text-muted">
        <span className="absolute -translate-x-1/2" style={{ left: UNLOCK_AT }}>
          {t("settings.clipUnlock")}
        </span>
      </div>
      <div className="relative">
        <Slider
          label={[t("settings.clipBeforeLabel"), t("settings.clipAfterLabel")]}
          min={START}
          max={END}
          step={1}
          value={[start, end]}
          onValueChange={(next) => setDraft(clampWindow(next, saved))}
          onValueCommit={(next) => {
            const [from, to] = clampWindow(next, saved);
            setDraft(null);
            onCommit(-from, to);
          }}
        />
        <span
          className="pointer-events-none absolute top-1/2 h-4 w-0.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-muted"
          style={{ left: UNLOCK_AT }}
        />
      </div>
      <div className="flex justify-between text-[12.5px] text-muted">
        <span>{t("settings.clipBefore", { n: -start })}</span>
        <span>{t("settings.clipAfter", { n: end })}</span>
      </div>
      <span className="text-[12.5px] text-dim">
        {t("settings.clipSize", { mb: clipSizeMb(end - start, fps, quality) })}
      </span>
    </div>
  );
}
