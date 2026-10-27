import { useState } from "react";
import { Button } from "../../../components/ui/button";
import { useI18n } from "../../../i18n/useI18n";

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
