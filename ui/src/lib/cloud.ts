import type { Key } from "../i18n/dictionary";
import type { CloudGameView, CloudLocated } from "../types/domain";

export interface FactText {
  key: Key;
  vars?: Record<string, string>;
}

export interface FactView {
  value: FactText;
  at: number | null;
  busy: boolean;
  alert: boolean;
}

const ERROR_KEYS = {
  missingRoot: "cloud.error.missingRoot",
  folderMissing: "cloud.error.folderMissing",
  pathNotPortable: "cloud.error.pathNotPortable",
  invalidName: "cloud.error.invalidName",
  multipleAccounts: "cloud.error.multipleAccounts",
  gameRunning: "cloud.error.gameRunning",
} as const satisfies Record<string, Key>;

export function cloudErrorKey(code: string): Key | null {
  return Object.hasOwn(ERROR_KEYS, code) ? ERROR_KEYS[code as keyof typeof ERROR_KEYS] : null;
}

export function cloudFactKey(view: CloudGameView, hasFolder: boolean): Key {
  if (!hasFolder) return "cloud.fact.setup";
  if (!view.enabled) return "cloud.fact.off";
  if (view.openElsewhere) return "cloud.card.openElsewhere";
  if (view.state && view.lastSnapshotAt !== null) return "cloud.status.synced";
  switch (view.located) {
    case "notFound":
      return "cloud.fact.notFound";
    case "registry":
    case "unsupported":
      return "cloud.fact.unsupported";
    case "waitingManifest":
      return "cloud.fact.waiting";
    case "found":
      return "cloud.fact.notSynced";
  }
}

const APP_FOLDER = "AchievementWatcher";

export function cloudFolderFor(picked: string): string {
  const trimmed = picked.replace(/[\\/]+$/, "");
  const name = trimmed.split(/[\\/]/).pop() ?? "";
  return name.toLowerCase() === APP_FOLDER.toLowerCase() ? trimmed : `${trimmed}\\${APP_FOLDER}`;
}

const PANEL_TITLES: Partial<Record<Key, Key>> = {
  "cloud.fact.off": "cloud.panel.title.off",
  "cloud.fact.notFound": "cloud.panel.title.notFound",
  "cloud.fact.unsupported": "cloud.panel.title.unsupported",
  "cloud.fact.waiting": "cloud.panel.title.waiting",
};

export function panelTitleKey(view: CloudGameView): Key {
  const fact = cloudFactKey(view, true);
  return PANEL_TITLES[fact] ?? fact;
}

const KEPT_FOLDERS = 2;

export function elidedPath(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  if (parts.length <= KEPT_FOLDERS) return parts.join("\\");
  return ["...", ...parts.slice(-KEPT_FOLDERS)].join("\\");
}

export function savePathAction(located: CloudLocated): "change" | "choose" | null {
  if (located === "found") return "change";
  if (located === "notFound") return "choose";
  return null;
}

export function gamesSavedKey(count: number): Key {
  if (count === 0) return "cloud.noGames";
  return count === 1 ? "cloud.gamesSavedOne" : "cloud.gamesSaved";
}

export function isSynced(view: CloudGameView): view is CloudGameView & { lastSnapshotAt: number } {
  return view.enabled && !view.openElsewhere && view.state !== null && view.lastSnapshotAt !== null;
}

export function cloudAttention(
  view: CloudGameView,
  seenAt: number | null,
): "branched" | "restored" | null {
  const action = view.lastAction;
  if (!action || (seenAt !== null && action.at <= seenAt)) return null;
  if (action.reason === "branch") return "branched";
  if (action.reason === "restore") return "restored";
  return null;
}

function fact(value: FactText, extra: Partial<FactView> = {}): FactView {
  return { value, at: null, busy: false, alert: false, ...extra };
}

export function statusFact(view: CloudGameView, hasFolder: boolean, syncingNow: boolean): FactView {
  const plain = () =>
    fact({ key: cloudFactKey(view, hasFolder), vars: { device: view.openElsewhere ?? "" } });
  if (!hasFolder || !view.enabled) return plain();
  const status = view.status;
  if (syncingNow || status.type === "syncing") {
    return fact({ key: "cloud.status.syncing" }, { busy: true });
  }
  if (status.type === "failed") return fact({ key: "cloud.status.failed" }, { alert: true });
  if (view.openElsewhere) return plain();
  switch (status.type) {
    case "waiting":
      return fact({ key: "cloud.status.waiting" });
    case "playing":
      return fact({ key: "cloud.status.playing" });
    case "behind":
      return fact({ key: "cloud.status.behind" });
    case "sent":
    case "received":
      return fact({ key: "cloud.status.synced" }, { at: status.at });
    case "idle":
      return plain();
  }
}

export interface StatusSummary {
  title: FactText;
  detail: FactText | null;
  at: number | null;
  retry: boolean;
}

function summary(title: FactText, detail: FactText | null = null, at: number | null = null) {
  return { title, detail, at, retry: false };
}

export function statusSummary(view: CloudGameView): StatusSummary | null {
  const status = view.status;
  switch (status.type) {
    case "syncing":
      return summary({ key: "cloud.status.syncing" });
    case "failed":
      return {
        ...summary(
          { key: "cloud.status.failed" },
          { key: cloudErrorKey(status.code) ?? "cloud.status.failedDetail" },
        ),
        retry: true,
      };
    case "waiting":
    case "playing":
    case "behind":
      return summary({ key: `cloud.status.${status.type}` });
    case "sent":
    case "received":
      return summary({ key: "cloud.status.synced" }, null, status.at);
    case "idle":
      return null;
  }
}

export function playtimeLabel(
  seconds: number | null,
): { key: "cloud.history.noHours" } | { key: "cloud.history.hours"; seconds: number } | null {
  if (seconds === null) return null;
  return seconds > 0 ? { key: "cloud.history.hours", seconds } : { key: "cloud.history.noHours" };
}

export type CloudMood = "idle" | "off" | "syncing" | "pulling" | "synced" | "failed" | "playing";

const SETTLED_MOODS = {
  failed: "failed",
  sent: "synced",
  received: "synced",
  behind: "pulling",
  waiting: "pulling",
  playing: "playing",
} as const satisfies Partial<Record<CloudGameView["status"]["type"], CloudMood>>;

export function cloudMood(view: CloudGameView, hasFolder: boolean, syncingNow: boolean): CloudMood {
  if (!hasFolder) return "idle";
  if (!view.enabled) return "off";
  const status = view.status.type;
  if (syncingNow || status === "syncing") return "syncing";
  if (Object.hasOwn(SETTLED_MOODS, status)) {
    return SETTLED_MOODS[status as keyof typeof SETTLED_MOODS];
  }
  if (view.openElsewhere) return "playing";
  return isSynced(view) ? "synced" : "idle";
}
