import { useState } from "react";
import { Icon } from "../../components/Icon";
import { Button } from "../../components/ui/button";
import { PLACEHOLDERS } from "../../constants";
import { useI18n } from "../../i18n/useI18n";

interface FolderListProps {
  label: string;
  values: string[];
  onChange: (values: string[]) => void;
}

export function FolderList({ label, values, onChange }: FolderListProps) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<string | null>(null);
  const add = () => {
    const path = draft?.trim() ?? "";
    if (path && !values.includes(path)) onChange([...values, path]);
    setDraft(null);
  };

  return (
    <div className="flex flex-col gap-1.5">
      {values.length === 0 && (
        <p className="rounded-card border border-line border-dashed p-3 text-[13px] text-dim">
          {t("settings.noFolders")}
        </p>
      )}
      {values.map((value) => (
        <div
          key={value}
          className="flex items-center gap-2 rounded-card border border-line bg-surface px-3 py-2"
        >
          <Icon name="folder" size={16} className="text-dim" />
          <span className="mr-auto break-all text-[13px]">{value}</span>
          <Button
            size="sm"
            variant="ghost"
            aria-label={t("settings.removeFolder", { path: value })}
            onClick={() => onChange(values.filter((item) => item !== value))}
          >
            <Icon name="delete" size={16} />
          </Button>
        </div>
      ))}
      {draft === null ? (
        <Button variant="link" size="sm" className="w-max" onClick={() => setDraft("")}>
          <Icon name="add" size={16} />
          {t("settings.addFolder")}
        </Button>
      ) : (
        <div className="flex gap-2">
          <input
            aria-label={label}
            value={draft}
            placeholder={PLACEHOLDERS.gamesFolder}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") add();
              if (event.key === "Escape") setDraft(null);
            }}
            className="flex-1 rounded-card border border-line bg-surface px-2.5 py-1.5 text-[13px] outline-none focus-visible:border-text"
          />
          <Button size="sm" variant="primary" onClick={add}>
            {t("settings.add")}
          </Button>
        </div>
      )}
    </div>
  );
}
