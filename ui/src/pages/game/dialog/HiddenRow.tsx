import { memo } from "react";
import { Icon } from "../../../components/Icon";
import { Button } from "../../../components/ui/button";
import { useI18n } from "../../../i18n/useI18n";
import type { AchievementRow } from "../../../types/domain";
import { DialogRow } from "./DialogRow";
import type { HiddenAction } from "./hidden";

interface HiddenRowProps {
  row: AchievementRow;
  revealed: boolean;
  dispatch: (action: HiddenAction) => void;
}

export const HiddenRow = memo(function HiddenRow({ row, revealed, dispatch }: HiddenRowProps) {
  const { t } = useI18n();
  const onReveal = () => dispatch({ type: "reveal", apiName: row.apiName });
  const onHide = () => dispatch({ type: "hide", apiName: row.apiName });
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
    <div className="dialog-row @container rounded-card border border-line bg-surface">
      <div className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-3.5 gap-y-1.5 px-3.5 py-2.5 @md:grid-cols-[auto_minmax(0,1fr)_auto]">
        <span className="row-span-2 grid size-10 place-items-center bg-surface-2 text-dim @md:row-span-1">
          <Icon name="eye-off" size={18} />
        </span>
        <div className="flex min-w-0 flex-col">
          <span className="font-[650] text-[14px] text-muted">{t("dialog.hiddenTitle")}</span>
          <span className="text-[12.5px] text-dim">{t("dialog.hiddenHint")}</span>
        </div>
        <div className="col-start-2 @md:col-start-3 @md:row-start-1">
          <Button size="sm" onClick={onReveal}>
            <Icon name="eye" size={15} />
            {t("dialog.reveal")}
          </Button>
        </div>
      </div>
    </div>
  );
});
