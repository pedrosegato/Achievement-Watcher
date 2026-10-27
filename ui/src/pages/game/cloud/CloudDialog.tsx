import { ErrorAlert } from "../../../components/ErrorAlert";
import { Icon } from "../../../components/Icon";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "../../../components/ui/dialog";
import { useCloudAction } from "../../../hooks/useCloudAction";
import { useI18n } from "../../../i18n/useI18n";
import { cloudErrorKey } from "../../../lib/cloud";
import type { CloudGameView } from "../../../types/domain";
import { AutoSync } from "./AutoSync";
import { CloudHistory } from "./CloudHistory";
import { CloudSummary } from "./CloudSummary";
import { SaveFolder } from "./SaveFolder";

interface CloudDialogProps {
  appId: number;
  gameName: string;
  view: CloudGameView;
  globalError: string | null;
  now: number;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function CloudDialog({
  appId,
  gameName,
  view,
  globalError,
  now,
  open,
  onOpenChange,
}: CloudDialogProps) {
  const { t } = useI18n();
  const { error, busy, run, clearError } = useCloudAction(appId);
  const shared =
    globalError && view.status.type !== "failed"
      ? t(cloudErrorKey(globalError) ?? "cloud.status.failedDetail")
      : null;
  const changeOpen = (next: boolean) => {
    if (!next) clearError();
    onOpenChange(next);
  };

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogContent className="w-[min(92vw,640px)]">
        <div className="flex items-start justify-between gap-4 border-line border-b px-6 pt-5 pb-4">
          <div className="flex min-w-0 flex-col gap-0.5">
            <DialogTitle className="font-bold text-[18px]">{t("cloud.card.title")}</DialogTitle>
            <DialogDescription className="truncate text-[13px] text-muted">
              {gameName}
            </DialogDescription>
          </div>
          <DialogClose asChild>
            <button
              type="button"
              aria-label={t("window.close")}
              className="grid size-8 flex-none cursor-pointer place-items-center rounded-card text-muted transition-colors duration-(--dur-fast) ease-(--ease-out) hover:bg-surface-2 hover:text-text"
            >
              <Icon name="close" size={16} />
            </button>
          </DialogClose>
        </div>
        <CloudSummary appId={appId} view={view} now={now} busy={busy} run={run} />
        {(error || shared) && (
          <div className="px-6 pb-3">
            <ErrorAlert message={error ?? shared} />
          </div>
        )}
        <CloudHistory appId={appId} snapshots={view.snapshots} now={now} busy={busy} run={run} />
        <SaveFolder appId={appId} view={view} busy={busy} run={run} />
        <AutoSync appId={appId} view={view} busy={busy} run={run} />
      </DialogContent>
    </Dialog>
  );
}
