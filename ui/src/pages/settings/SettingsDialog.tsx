import { Icon } from "../../components/Icon";
import { Dialog, DialogClose, DialogContent } from "../../components/ui/dialog";
import { useI18n } from "../../i18n/useI18n";
import { SettingsBody } from "./SettingsBody";

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  topic?: string;
}

export function SettingsDialog({ open, onOpenChange, topic = "general" }: SettingsDialogProps) {
  const { t } = useI18n();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        aria-describedby={undefined}
        className="h-[min(720px,calc(100vh-var(--titlebar)-48px))] max-h-none w-[min(980px,calc(100vw-48px))] bg-bg"
      >
        <SettingsBody topic={topic} />
        <DialogClose asChild>
          <button
            type="button"
            aria-label={t("window.close")}
            className="absolute top-3 right-3 grid size-8 cursor-pointer place-items-center rounded-card bg-bg text-muted transition-colors duration-(--dur-fast) ease-(--ease-out) hover:bg-surface-2 hover:text-text"
          >
            <Icon name="close" size={16} />
          </button>
        </DialogClose>
      </DialogContent>
    </Dialog>
  );
}
