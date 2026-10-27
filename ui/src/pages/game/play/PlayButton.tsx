import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../../../components/ui/button";
import { Confirm } from "../../../components/ui/confirm";
import { useChanged } from "../../../hooks/useChanged";
import { usePlayMoment } from "../../../hooks/usePlayMoment";
import { usePlayView } from "../../../hooks/usePlayView";
import type { Key } from "../../../i18n/dictionary";
import { useI18n } from "../../../i18n/useI18n";
import { api, errorMessage } from "../../../lib/commands";
import { pickFile } from "../../../lib/dialog";
import { formatWhen } from "../../../lib/format";
import { type PlayMoment, type PlayPhase, playErrorKey } from "../../../lib/play";
import { queries } from "../../../lib/queries";
import type { PlayState } from "../../../types/domain";
import { LaunchMenu } from "./LaunchMenu";
import { PlayContent } from "./PlayContent";
import { PlayNotice } from "./PlayNotice";
import { PlayPlaceholder } from "./PlayPlaceholder";
import { StuckNote } from "./StuckNote";
import { MAIN } from "./styles";

type Prompt = { kind: "elsewhere"; device: string; since: number } | { kind: "stop" };

function elsewhereOf(
  prompt: Prompt | null,
  state: PlayState,
): { device: string; since: number } | null {
  if (prompt?.kind === "elsewhere") return prompt;
  if (state.type === "openElsewhere") return { device: state.device, since: state.since };
  return null;
}

const SYNCING = {
  variant: "secondary",
  motion: "btn-sheen aria-disabled:hover:bg-surface",
} as const;

const LOOKS: Record<PlayPhase, { variant: "secondary" | "stop" | "play"; motion: string }> = {
  ready: { variant: "play", motion: "" },
  pickExe: { variant: "play", motion: "" },
  pulling: SYNCING,
  pushing: SYNCING,
  launching: { variant: "play", motion: "btn-glow aria-disabled:hover:bg-play" },
  running: { variant: "stop", motion: "" },
  closing: { variant: "secondary", motion: "aria-disabled:hover:bg-surface" },
};

const MOMENT_MOTION: Record<PlayMoment, string> = {
  rumble: "btn-rumble",
  synced: "btn-pop btn-flash",
};

export function PlayButton({ appId }: { appId: number }) {
  const { t, locale } = useI18n();
  const { data: state, dataUpdatedAt } = useQuery(queries.playState(appId));
  const [prompt, setPrompt] = useState<Prompt | null>(null);
  const [error, setError] = useState<Key | null>(null);
  const { view, markClosing } = usePlayView(state);
  const relabelled = useChanged(view?.phase);
  const moment = usePlayMoment(view?.phase);

  if (!state || !view) return <PlayPlaceholder />;
  const now = Math.floor(dataUpdatedAt / 1000);

  const run = async (work: () => Promise<void>, fallback: Key = "play.failed.generic") => {
    setError(null);
    try {
      await work();
    } catch (failure) {
      setError(playErrorKey(errorMessage(failure), fallback));
    }
  };

  const start = async (confirmed: boolean): Promise<void> => {
    const response = await api.playGame(appId, confirmed);
    if (response.type === "openElsewhere") {
      setPrompt({ kind: "elsewhere", device: response.device, since: response.since });
    } else if (response.type === "needsExe") {
      const path = await pickFile(t("play.exe.pickTitle"), ["exe"]);
      if (path) {
        await api.setLaunchExe(appId, path);
        await start(false);
      }
    }
  };

  const onClick = () => {
    if (view.disabled) return;
    if (view.action === "stop") setPrompt({ kind: "stop" });
    else if (view.action !== "none") void run(() => start(false), "play.failed.launchFailed");
  };

  const close = () => setPrompt(null);
  const elsewhere = elsewhereOf(prompt, state);
  const failure =
    state.type === "failed" ? playErrorKey(state.reason, "play.failed.launchFailed") : error;
  const message = failure ? t(failure) : null;
  const look = LOOKS[view.phase];

  return (
    <div className="relative">
      <div className="flex gap-px">
        <Button
          variant={look.variant}
          aria-disabled={view.disabled}
          aria-busy={view.busy}
          onClick={onClick}
          className={`${MAIN} group aria-disabled:cursor-default ${look.motion} ${moment ? MOMENT_MOTION[moment] : ""}`}
        >
          <PlayContent
            phase={view.phase}
            labelKey={view.labelKey}
            moment={moment}
            relabelled={relabelled}
          />
        </Button>
        <LaunchMenu appId={appId} variant={look.variant} disabled={view.busy} run={run} />
      </div>
      {view.stuck && (
        <StuckNote
          onCancel={() => void run(() => api.cancelPlay(appId))}
          onPlayAnyway={() => void run(() => api.playAnyway(appId))}
        />
      )}
      {message && !view.busy && !view.stuck && (
        <PlayNotice
          text={message}
          onDismiss={() => {
            setError(null);
            if (state.type === "failed") void run(() => api.cancelPlay(appId));
          }}
        />
      )}
      {elsewhere && (
        <Confirm
          title={t("play.elsewhere.title", {
            device: elsewhere.device,
            time: formatWhen(elsewhere.since, locale, now),
          })}
          body={t("play.elsewhere.body")}
          confirm={t("play.elsewhere.confirm")}
          secondary={t("play.cancel")}
          onConfirm={() => {
            close();
            void run(() => start(true), "play.failed.launchFailed");
          }}
          onSecondary={() =>
            prompt?.kind === "elsewhere" ? close() : void run(() => api.cancelPlay(appId))
          }
        />
      )}
      {prompt?.kind === "stop" && (
        <Confirm
          title={t("play.stop.title")}
          body={t("play.stop.body")}
          confirm={t("play.stop.confirm")}
          secondary={t("play.cancel")}
          onConfirm={() => {
            close();
            markClosing(true);
            void run(
              () =>
                api.stopGame(appId).catch((failure: unknown) => {
                  markClosing(false);
                  throw failure;
                }),
              "play.failed.stopFailed",
            );
          }}
          onSecondary={close}
        />
      )}
    </div>
  );
}
