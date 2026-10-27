import { useQuery } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { Icon } from "../../components/Icon";
import { Button } from "../../components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "../../components/ui/dialog";
import { EVENTS } from "../../constants";
import { useTauriEvent } from "../../hooks/useTauriEvent";
import type { Key } from "../../i18n/dictionary";
import { useI18n } from "../../i18n/useI18n";
import { api, errorMessage } from "../../lib/commands";
import { pickFile } from "../../lib/dialog";
import { formatTime } from "../../lib/format";
import { type PlayButtonView, playButtonView } from "../../lib/play";
import { queries } from "../../lib/queries";
import type { PlayState } from "../../types/domain";
import { LaunchMenu } from "./LaunchMenu";

type Prompt = { kind: "elsewhere"; device: string; since: number } | { kind: "stop" };

const MESSAGES: Record<string, Key> = {
  launchUnconfirmed: "play.failed.launchUnconfirmed",
  launchFailed: "play.failed.launchFailed",
  notAnExe: "play.error.notAnExe",
};

interface ConfirmProps {
  title: string;
  body: string;
  confirm: string;
  secondary: string;
  onConfirm: () => void;
  onSecondary: () => void;
  onClose?: () => void;
}

function Confirm({
  title,
  body,
  confirm,
  secondary,
  onConfirm,
  onSecondary,
  onClose,
}: ConfirmProps) {
  return (
    <Dialog open onOpenChange={(open) => !open && (onClose ?? onSecondary)()}>
      <DialogContent className="w-[min(92vw,440px)] px-6 pt-5 pb-5">
        <DialogTitle className="font-bold text-[16px]">{title}</DialogTitle>
        <DialogDescription className="mt-1.5 break-words text-[13px] text-muted">
          {body}
        </DialogDescription>
        <div className="mt-5 flex justify-end gap-2">
          <Button onClick={onSecondary}>{secondary}</Button>
          <Button variant="primary" onClick={onConfirm}>
            {confirm}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

const FLOAT =
  "pop-surface absolute top-full left-0 z-20 mt-2 w-max max-w-80 rounded-card border border-line bg-raised text-text shadow-raised";

function StuckNote({ onCancel, onPlayAnyway }: { onCancel: () => void; onPlayAnyway: () => void }) {
  const { t } = useI18n();
  return (
    <div
      role="alert"
      data-state="open"
      className={`${FLOAT} flex flex-col items-start gap-2.5 px-3.5 py-3`}
    >
      <p className="text-[13px]">{t("play.stuck.text")}</p>
      <div className="flex gap-2">
        <Button size="sm" onClick={onCancel}>
          {t("play.cancel")}
        </Button>
        <Button size="sm" variant="primary" onClick={onPlayAnyway}>
          {t("play.stuck.playAnyway")}
        </Button>
      </div>
    </div>
  );
}

function Notice({ text, onDismiss }: { text: string; onDismiss: () => void }) {
  const { t } = useI18n();
  return (
    <div
      role="status"
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

function elsewhereOf(
  prompt: Prompt | null,
  state: PlayState,
): { device: string; since: number } | null {
  if (prompt?.kind === "elsewhere") return prompt;
  if (state.type === "openElsewhere") return { device: state.device, since: state.since };
  return null;
}

function lookOf(view: PlayButtonView): {
  variant: "secondary" | "stop" | "play";
  icon: "close" | "play";
} {
  if (view.busy) return { variant: "secondary", icon: "play" };
  if (view.action === "stop") return { variant: "stop", icon: "close" };
  return { variant: "play", icon: "play" };
}

function Spinner() {
  return (
    <span
      aria-hidden
      className="spinning size-4 rounded-full border-2 border-current border-r-transparent"
    />
  );
}

export function PlayButton({ appId }: { appId: number }) {
  const { t, locale } = useI18n();
  const { data: state, refetch } = useQuery(queries.playState(appId));
  const refresh = useCallback(() => void refetch(), [refetch]);
  useTauriEvent(EVENTS.playChanged, refresh);
  useTauriEvent(EVENTS.gamesChanged, refresh);
  const [prompt, setPrompt] = useState<Prompt | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (!state) return null;
  const view = playButtonView(state);

  const run = async (work: () => Promise<void>) => {
    setError(null);
    try {
      await work();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      refresh();
    }
  };

  const start = (confirmed: boolean): Promise<void> =>
    run(async () => {
      const response = await api.playGame(appId, confirmed);
      if (response.type === "openElsewhere") {
        setPrompt({ kind: "elsewhere", device: response.device, since: response.since });
      } else if (response.type === "needsExe") {
        await chooseExe();
      }
    });

  const applyExe = (path: string) =>
    run(async () => {
      await api.setLaunchExe(appId, path);
      await start(false);
    });

  async function chooseExe() {
    const path = await pickFile(t("play.exe.pickTitle"), ["exe"]);
    if (path) await applyExe(path);
  }

  const onClick = () => {
    if (view.action === "play") void start(false);
    else if (view.action === "pickExe") void run(() => chooseExe());
    else if (view.action === "stop") setPrompt({ kind: "stop" });
  };

  const close = () => setPrompt(null);
  const elsewhere = elsewhereOf(prompt, state);
  const failure = state.type === "failed" ? state.reason : error;
  const message = failure && (MESSAGES[failure] ? t(MESSAGES[failure]) : failure);
  const look = lookOf(view);

  return (
    <div className="relative">
      <div className="flex gap-px">
        <Button
          variant={look.variant}
          disabled={view.disabled}
          aria-busy={view.busy}
          onClick={onClick}
          className="h-11 min-w-48 rounded-r-none px-6 font-bold text-[15px] disabled:pointer-events-none disabled:bg-surface-2 disabled:opacity-100"
        >
          {view.busy ? <Spinner /> : <Icon name={look.icon} size={18} />}
          {t(view.labelKey)}
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
        <Notice
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
            time: formatTime(elsewhere.since, locale),
          })}
          body={t("play.elsewhere.body")}
          confirm={t("play.elsewhere.confirm")}
          secondary={t("play.cancel")}
          onConfirm={() => {
            close();
            void start(true);
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
            void run(() => api.stopGame(appId));
          }}
          onSecondary={close}
        />
      )}
    </div>
  );
}
