import type { Key } from "../i18n/dictionary";
import type { UpdateStatus } from "../types/domain";
import type { Vars } from "../types/ui";

interface Text {
  key: Key;
  vars?: Vars;
}

export interface UpdateView {
  busy: boolean;
  action: Text;
  note: Text | null;
  percent: number | null;
}

const PERCENT = 100;

export function updatePercent(status: UpdateStatus): number | null {
  if (status.phase === "installing") return PERCENT;
  if (status.phase !== "downloading" || !status.total) return null;
  return Math.min(PERCENT, Math.floor((status.downloaded * PERCENT) / status.total));
}

export function updateView(status: UpdateStatus): UpdateView {
  const percent = updatePercent(status);
  const version = { version: status.current };
  switch (status.phase) {
    case "checking":
      return { busy: true, action: { key: "settings.updateChecking" }, note: null, percent };
    case "downloading":
      return {
        busy: true,
        action:
          percent === null
            ? { key: "settings.updateDownloadingStart" }
            : { key: "settings.updateDownloading", vars: { pct: percent } },
        note: { key: "settings.updateDownloadingNote", vars: { version: status.available ?? "" } },
        percent,
      };
    case "installing":
      return {
        busy: true,
        action: { key: "settings.updateInstalling" },
        note: { key: "settings.updateRestarting" },
        percent,
      };
    case "idle":
      break;
  }
  const action: Text = status.available
    ? { key: "settings.installUpdate", vars: { version: status.available } }
    : { key: "settings.checkUpdate" };
  let note: Text = { key: "settings.version", vars: version };
  if (status.error) note = { key: "settings.updateError", vars: { error: status.error } };
  else if (status.available) {
    note = { key: "settings.updateAvailable", vars: { ...version, next: status.available } };
  } else if (status.checkedAt) note = { key: "settings.versionCurrent", vars: version };
  return { busy: false, action, note, percent };
}
