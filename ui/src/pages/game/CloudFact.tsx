import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Icon } from "../../components/Icon";
import { EVENTS } from "../../constants";
import { useCloudAction } from "../../hooks/useCloudAction";
import { useSeenAt } from "../../hooks/useSeenAt";
import { useTauriEvent } from "../../hooks/useTauriEvent";
import { useI18n } from "../../i18n/useI18n";
import { cloudAttention, cloudFactKey, isSynced } from "../../lib/cloud";
import { formatWhen } from "../../lib/format";
import { queries } from "../../lib/queries";
import { CloudDialog } from "./CloudDialog";

interface CloudFactProps {
  appId: number;
  gameName: string;
  onOpenSettings: () => void;
}

const TRIGGER =
  "-my-1.5 ml-auto flex min-w-0 cursor-pointer items-center gap-2.5 rounded-card border border-line bg-surface py-1.5 pr-3.5 pl-3 text-left transition-colors duration-(--dur-fast) ease-(--ease-out) hover:border-dim hover:bg-surface-2";

export function CloudFact({ appId, gameName, onOpenSettings }: CloudFactProps) {
  const { t, locale } = useI18n();
  const [now] = useState(() => Math.floor(Date.now() / 1000));
  const [open, setOpen] = useState(false);
  const [seenAt, markSeen] = useSeenAt(`cloud-seen:${appId}`);
  const { data: overview } = useQuery(queries.cloudOverview());
  const { data: view } = useQuery(queries.cloudGame(appId));
  const { refresh } = useCloudAction();
  useTauriEvent(EVENTS.cloudChanged, () => void refresh());

  if (!overview || !view) return null;
  const hasFolder = overview.folder !== null;
  const attention = hasFolder ? cloudAttention(view, seenAt) : null;
  const value = attention
    ? t(attention === "branched" ? "cloud.fact.branched" : "cloud.fact.restored")
    : isSynced(view)
      ? formatWhen(view.lastSnapshotAt, locale, now)
      : t(cloudFactKey(view, hasFolder), { device: view.openElsewhere ?? "" });
  const openDialog = () => {
    if (view.lastAction) markSeen(view.lastAction.at);
    setOpen(true);
  };

  return (
    <>
      <button type="button" className={TRIGGER} onClick={hasFolder ? openDialog : onOpenSettings}>
        <span className="relative flex-none">
          <Icon name="cloud" size={22} className={attention ? "text-text" : "text-dim"} />
          {attention && (
            <span className="absolute -top-0.5 -right-0.5 size-2 rounded-full bg-text ring-2 ring-bg" />
          )}
        </span>
        <span className="flex min-w-0 flex-col">
          <span className="text-[11.5px] text-dim">{t("cloud.card.title")}</span>
          <span className="truncate font-semibold text-[14px]">{value}</span>
        </span>
      </button>
      {hasFolder && (
        <CloudDialog
          appId={appId}
          gameName={gameName}
          view={view}
          thisPc={overview.deviceName}
          now={now}
          open={open}
          onOpenChange={setOpen}
        />
      )}
    </>
  );
}
