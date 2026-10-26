import { useEffect, useState } from "react";
import { EVENTS } from "../../constants";
import { useTauriEvent } from "../../hooks/useTauriEvent";
import { api } from "../../lib/commands";
import { ffmpegAction, ffmpegNote } from "../../lib/ffmpeg";
import type { FfmpegStatus } from "../../types/domain";
import type { UpdateSetting } from "../../types/ui";

export function useFfmpeg(update: UpdateSetting) {
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

  const startDownload = (current: FfmpegStatus) => {
    setStatus({ ...current, downloading: true, downloaded: 0, total: null, error: null });
    api.downloadFfmpeg().catch(() => undefined);
  };

  const setVideo = (on: boolean) => {
    void update("video", on);
    const action = ffmpegAction(on, status);
    if (action === "cancel") api.cancelFfmpeg().catch(() => undefined);
    if (action === "download" && status) startDownload(status);
    if (action === "fetch") {
      api
        .ffmpegStatus()
        .then((fresh) => {
          setStatus(fresh);
          if (ffmpegAction(true, fresh) === "download") startDownload(fresh);
        })
        .catch(() => undefined);
    }
  };

  return { note: ffmpegNote(status), downloading, setVideo };
}
