import { type KeyboardEvent, useState } from "react";
import type { ValueRange } from "../../../types/ui";

interface NumberFieldProps {
  label: string;
  value: number;
  range: ValueRange;
  suffix: string;
  onCommit: (value: number) => void;
}

export function NumberField({ label, value, range, suffix, onCommit }: NumberFieldProps) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    const parsed = Number.parseFloat(draft ?? "");
    setDraft(null);
    if (Number.isNaN(parsed)) return;
    const next = Math.min(range.max, Math.max(range.min, parsed));
    if (next !== value) onCommit(next);
  };
  const onKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") commit();
  };
  return (
    <span className="flex items-center gap-2 text-[13px] text-muted">
      <input
        aria-label={label}
        inputMode="decimal"
        value={draft ?? String(value)}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={onKey}
        className="w-11 rounded-card border border-line bg-surface py-1.5 text-center font-semibold text-[13px] text-text outline-none focus-visible:border-text"
      />
      <span>{suffix}</span>
    </span>
  );
}
