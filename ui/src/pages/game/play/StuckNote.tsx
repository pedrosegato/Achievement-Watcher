import { Button } from "../../../components/ui/button";
import { useI18n } from "../../../i18n/useI18n";
import { FLOAT } from "./styles";

export function StuckNote({
  onCancel,
  onPlayAnyway,
}: {
  onCancel: () => void;
  onPlayAnyway: () => void;
}) {
  const { t } = useI18n();
  return (
    <div role="alert" data-state="open" className={`${FLOAT} flex flex-col gap-2.5 px-3.5 py-3`}>
      <div className="flex flex-col gap-1">
        <p className="font-semibold text-[13px]">{t("play.stuck.text")}</p>
        <p className="text-[12.5px] text-muted">{t("play.stuck.risk")}</p>
      </div>
      <div className="flex gap-2">
        <Button size="sm" onClick={onCancel}>
          {t("play.dontOpen")}
        </Button>
        <Button size="sm" variant="primary" onClick={onPlayAnyway}>
          {t("play.stuck.playAnyway")}
        </Button>
      </div>
    </div>
  );
}
