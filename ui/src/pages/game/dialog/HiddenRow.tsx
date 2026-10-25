import { Icon } from "../../../components/Icon";
import { Button } from "../../../components/ui/button";
import { useI18n } from "../../../i18n/useI18n";
import type { AchievementRow } from "../../../types/domain";
import { DialogRow } from "./DialogRow";

interface HiddenRowProps {
  row: AchievementRow;
  revealed: boolean;
  onReveal: () => void;
  onHide: () => void;
}

export function HiddenRow({ row, revealed, onReveal, onHide }: HiddenRowProps) {
  const { t } = useI18n();
  if (revealed) {
    return (
      <div className="reveal-in">
        <DialogRow
          row={row}
          locked
          aside={
            <Button size="sm" onClick={onHide}>
              {t("dialog.hide")}
            </Button>
          }
        />
      </div>
    );
  }
  return (
    <div className="flex items-center gap-3.5 rounded-card border border-line bg-surface px-3.5 py-2.5">
      <span className="grid size-10 flex-none place-items-center bg-surface-2 text-dim">
        <Icon name="eye-off" size={18} />
      </span>
      <div className="mr-auto flex min-w-0 flex-col">
        <span className="font-[650] text-[14px] text-muted">{t("dialog.hiddenTitle")}</span>
        <span className="text-[12.5px] text-dim">{t("dialog.hiddenHint")}</span>
      </div>
      <Button size="sm" onClick={onReveal}>
        <Icon name="eye" size={15} />
        {t("dialog.reveal")}
      </Button>
    </div>
  );
}
