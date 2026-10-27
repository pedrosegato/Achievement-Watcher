import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { CloudGlyph } from "../../../components/CloudGlyph";
import { useCloudActivity } from "../../../hooks/useCloudActivity";
import { useSeenAt } from "../../../hooks/useSeenAt";
import type { Key } from "../../../i18n/dictionary";
import { useI18n } from "../../../i18n/useI18n";
import { cloudAttention, cloudMood, statusFact } from "../../../lib/cloud";
import { formatWhen } from "../../../lib/format";
import { queries } from "../../../lib/queries";
import type { CloudGameView } from "../../../types/domain";
import { CloudDialog } from "./CloudDialog";

interface CloudFactProps {
  appId: number;
  gameName: string;
  onOpenSettings: () => void;
}

const TRIGGER =
  "-my-1.5 ml-auto flex min-w-0 cursor-pointer items-center gap-2.5 rounded-card border border-line bg-surface py-1.5 pr-3.5 pl-3 text-left transition-colors duration-(--dur-fast) ease-(--ease-out) hover:border-dim hover:bg-surface-2";

interface Described {
  value: string;
  time: string | null;
  flagged: boolean;
}

function describe(
  view: CloudGameView,
  hasFolder: boolean,
  syncingNow: boolean,
  seenAt: number | null,
  t: (key: Key, vars?: Record<string, string>) => string,
  when: (at: number) => string,
): Described {
  const shown = statusFact(view, hasFolder, syncingNow);
  const attention = hasFolder && shown.at !== null ? cloudAttention(view, seenAt) : null;
  if (attention) {
    const key = attention === "branched" ? "cloud.fact.branched" : "cloud.fact.restored";
    return { value: t(key), time: null, flagged: true };
  }
  return {
    value: t(shown.value.key, shown.value.vars),
    time: shown.at !== null ? when(shown.at) : null,
    flagged: shown.alert,
  };
}

export function CloudFact({ appId, gameName, onOpenSettings }: CloudFactProps) {
  const { t, locale } = useI18n();
  const [open, setOpen] = useState(false);
  const [seenAt, markSeen] = useSeenAt(`cloud-seen:${appId}`);
  const { data: overview } = useQuery(queries.cloudOverview());
  const { data: view, dataUpdatedAt } = useQuery(queries.cloudGame(appId));
  const syncingNow = useCloudActivity(appId);

  if (!overview || !view) return null;
  const now = Math.floor(dataUpdatedAt / 1000);
  const hasFolder = overview.folder !== null;
  const { value, time, flagged } = describe(view, hasFolder, syncingNow, seenAt, t, (at) =>
    formatWhen(at, locale, now),
  );
  const mood = cloudMood(view, hasFolder, syncingNow);
  const tone =
    mood === "idle" || mood === "off" ? "text-dim" : flagged ? "text-text" : "text-muted";
  const openDialog = () => {
    if (view.lastAction) markSeen(view.lastAction.at);
    setOpen(true);
  };

  return (
    <>
      <button
        type="button"
        aria-haspopup={hasFolder ? "dialog" : undefined}
        className={TRIGGER}
        onClick={hasFolder ? openDialog : onOpenSettings}
      >
        <span className="relative grid size-[22px] flex-none place-items-center">
          <CloudGlyph mood={mood} className={tone} />
          {flagged && (
            <span className="absolute -top-0.5 -right-0.5 size-2 rounded-full bg-text ring-2 ring-surface" />
          )}
        </span>
        <span className="flex min-w-0 flex-col">
          <span className="text-[11.5px] text-dim">{t("cloud.card.title")}</span>
          <span aria-live="polite" className="flex min-w-0 items-baseline gap-1.5">
            <span className="truncate font-semibold text-[14px]">{value}</span>
            {time && <span className="flex-none text-[13px] text-muted">{time}</span>}
          </span>
        </span>
      </button>
      {hasFolder && (
        <CloudDialog
          appId={appId}
          gameName={gameName}
          view={view}
          globalError={overview.lastError}
          now={now}
          open={open}
          onOpenChange={setOpen}
        />
      )}
    </>
  );
}
