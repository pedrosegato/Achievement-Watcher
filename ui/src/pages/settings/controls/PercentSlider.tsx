import { useState } from "react";
import { Slider } from "../../../components/ui/slider";
import type { ValueRange } from "../../../types/ui";

const PERCENT = 100;

interface PercentSliderProps {
  label: string;
  value: number;
  range: ValueRange;
  onCommit: (value: number) => void;
}

export function PercentSlider({ label, value, range, onCommit }: PercentSliderProps) {
  const [draft, setDraft] = useState<number | null>(null);
  const shown = draft ?? value;
  return (
    <div className="flex items-center gap-2.5">
      <Slider
        label={label}
        className="w-[180px]"
        min={range.min}
        max={range.max}
        step={range.step}
        value={[shown]}
        onValueChange={([next]) => next !== undefined && setDraft(next)}
        onValueCommit={([next]) => {
          setDraft(null);
          if (next !== undefined) onCommit(next);
        }}
      />
      <span className="w-[42px] text-right text-[13px] text-muted">
        {Math.round(shown * PERCENT)}%
      </span>
    </div>
  );
}
