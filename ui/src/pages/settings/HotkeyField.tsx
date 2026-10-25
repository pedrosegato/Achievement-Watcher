import { useEffect, useEffectEvent, useState } from "react";
import { Button } from "../../components/ui/button";
import { useI18n } from "../../i18n/useI18n";
import { comboFromKey, splitCombo } from "./hotkey";

interface HotkeyFieldProps {
  value: string;
  onChange: (combo: string) => void;
}

export function HotkeyField({ value, onChange }: HotkeyFieldProps) {
  const { t } = useI18n();
  const [recording, setRecording] = useState(false);
  const commit = useEffectEvent((combo: string) => onChange(combo));

  useEffect(() => {
    if (!recording) return;
    const onKey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        setRecording(false);
        return;
      }
      const combo = comboFromKey(event);
      if (combo) {
        setRecording(false);
        commit(combo);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording]);

  return (
    <>
      {recording ? (
        <span className="text-[13px] text-muted">{t("settings.hotkeyRecording")}</span>
      ) : (
        <span className="flex gap-1">
          {splitCombo(value).map((part) => (
            <kbd
              key={part}
              className="rounded-card border border-line border-b-2 bg-surface px-2 py-1 font-semibold text-[12px]"
            >
              {part}
            </kbd>
          ))}
        </span>
      )}
      <Button size="sm" onClick={() => setRecording((current) => !current)}>
        {recording ? t("settings.cancel") : t("settings.change")}
      </Button>
    </>
  );
}
