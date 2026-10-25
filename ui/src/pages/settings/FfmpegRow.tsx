import { useEffect, useState } from "react";
import { Button } from "../../components/ui/button";
import { Switch } from "../../components/ui/switch";
import { EVENTS } from "../../constants";
import { useTauriEvent } from "../../hooks/useTauriEvent";
import { useI18n } from "../../i18n/useI18n";
import { api } from "../../lib/commands";
import type { FfmpegStatus, Settings } from "../../types/domain";
import type { UpdateSetting } from "../../types/ui";
import { SettingRow } from "./SettingRow";

interface FfmpegRowProps {
  settings: Settings;
  update: UpdateSetting;
}

const BYTES_PER_MB = 1_000_000;
const PERCENT = 100;

export function FfmpegRow({ settings, update }: FfmpegRowProps) {
  const { t } = useI18n();
  const [status, setStatus] = useState<FfmpegStatus | null>(null);

  useEffect(() => {
    let current = true;
    api
      .ffmpegStatus()
      .then((value) => {
        if (current) setStatus(value);
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, []);

  useTauriEvent<FfmpegStatus>(EVENTS.ffmpegProgress, setStatus);

  const downloading = status?.downloading ?? false;

  const toggle = (on: boolean) => {
    void update("video", on);
    if (!on && downloading) {
      api.cancelFfmpeg().catch(() => undefined);
      return;
    }
    if (on && status && !status.available && !downloading) {
      setStatus({ ...status, downloading: true, downloaded: 0, total: null, error: null });
      api.downloadFfmpeg().catch(() => undefined);
    }
  };

  let hint = t("settings.videoFirst");
  if (downloading && status?.total) {
    hint = t("settings.ffmpegProgress", {
      got: Math.round(status.downloaded / BYTES_PER_MB),
      total: Math.round(status.total / BYTES_PER_MB),
      pct: Math.floor((status.downloaded * PERCENT) / status.total),
    });
  } else if (downloading) {
    hint = t("settings.ffmpegStarting");
  } else if (status?.error) {
    hint = t("settings.ffmpegError", { error: status.error });
  } else if (status?.available) {
    hint = settings.video ? t("settings.videoReady") : t("settings.videoIdle");
  }

  return (
    <SettingRow label={t("settings.video")} hint={hint}>
      {downloading && (
        <Button size="sm" onClick={() => toggle(false)}>
          {t("settings.cancel")}
        </Button>
      )}
      <Switch aria-label={t("settings.video")} checked={settings.video} onCheckedChange={toggle} />
    </SettingRow>
  );
}
