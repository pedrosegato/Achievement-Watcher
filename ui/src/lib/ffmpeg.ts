import type { Key } from "../i18n/dictionary";
import type { FfmpegStatus } from "../types/domain";
import type { Vars } from "../types/ui";

interface Note {
  key: Key;
  vars?: Vars;
}

const BYTES_PER_MB = 1_000_000;
const PERCENT = 100;

export function ffmpegNote(status: FfmpegStatus | null): Note | null {
  if (!status) return null;
  if (status.downloading && status.total) {
    return {
      key: "settings.ffmpegProgress",
      vars: {
        got: Math.round(status.downloaded / BYTES_PER_MB),
        total: Math.round(status.total / BYTES_PER_MB),
        pct: Math.floor((status.downloaded * PERCENT) / status.total),
      },
    };
  }
  if (status.downloading) return { key: "settings.ffmpegStarting" };
  if (status.error) return { key: "settings.ffmpegError", vars: { error: status.error } };
  if (!status.available) return { key: "settings.videoFirst" };
  return null;
}
