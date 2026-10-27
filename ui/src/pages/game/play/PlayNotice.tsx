import { Icon } from "../../../components/Icon";
import { Button } from "../../../components/ui/button";
import { useI18n } from "../../../i18n/useI18n";
import { FLOAT } from "./styles";

export function PlayNotice({ text, onDismiss }: { text: string; onDismiss: () => void }) {
  const { t } = useI18n();
  return (
    <div
      role="alert"
      data-state="open"
      className={`${FLOAT} flex items-center gap-2 py-1.5 pr-1.5 pl-3.5`}
    >
      <p className="text-[13px]">{text}</p>
      <Button variant="ghost" size="icon" aria-label={t("window.close")} onClick={onDismiss}>
        <Icon name="close" size={15} />
      </Button>
    </div>
  );
}
