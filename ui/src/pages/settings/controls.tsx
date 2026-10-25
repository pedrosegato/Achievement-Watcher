import { type KeyboardEvent, useState } from "react";
import { Icon } from "../../components/Icon";
import { Button } from "../../components/ui/button";
import { Slider } from "../../components/ui/slider";
import { useI18n } from "../../i18n/useI18n";
import { api } from "../../lib/commands";
import type { UserFolder } from "../../types/domain";

interface Range {
  min: number;
  max: number;
  step: number;
}

const PERCENT = 100;

interface PercentSliderProps {
  label: string;
  value: number;
  range: Range;
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

interface NumberFieldProps {
  label: string;
  value: number;
  range: Range;
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

interface PathFieldProps {
  label: string;
  value: string | null;
  fallback: string;
  onCommit: (value: string | null) => void;
}

export function PathField({ label, value, fallback, onCommit }: PathFieldProps) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<string | null>(null);
  if (draft === null) {
    return (
      <>
        <span className="max-w-[260px] truncate text-[13px] text-muted">{value ?? fallback}</span>
        <Button size="sm" onClick={() => setDraft(value ?? "")}>
          {t("settings.change")}
        </Button>
      </>
    );
  }
  const save = () => {
    onCommit(draft.trim() || null);
    setDraft(null);
  };
  return (
    <>
      <input
        aria-label={label}
        value={draft}
        placeholder={fallback}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") save();
          if (event.key === "Escape") setDraft(null);
        }}
        className="w-[260px] rounded-card border border-line bg-surface px-2.5 py-1.5 text-[13px] outline-none focus-visible:border-text"
      />
      <Button size="sm" variant="primary" onClick={save}>
        {t("settings.save")}
      </Button>
    </>
  );
}

interface FolderLinkProps {
  folder: UserFolder;
  label: string;
}

export function FolderLink({ folder, label }: FolderLinkProps) {
  return (
    <Button
      variant="link"
      size="sm"
      className="w-max"
      onClick={() => {
        api.openUserFolder(folder).catch(() => undefined);
      }}
    >
      <Icon name="folder-open" size={16} />
      {label}
    </Button>
  );
}
