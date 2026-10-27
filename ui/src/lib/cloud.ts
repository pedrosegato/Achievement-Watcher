import type { Key } from "../i18n/dictionary";
import type { CloudGameView, CloudLocated, HeadReason } from "../types/domain";

const ERROR_KEYS = {
  missingRoot: "cloud.error.missingRoot",
  folderMissing: "cloud.error.folderMissing",
  pathNotPortable: "cloud.error.pathNotPortable",
  invalidName: "cloud.error.invalidName",
  multipleAccounts: "cloud.error.multipleAccounts",
  gameRunning: "cloud.error.gameRunning",
} as const satisfies Record<string, Key>;

const ACTION_KEYS = {
  push: "cloud.card.lastAction.push",
  restore: "cloud.card.lastAction.restore",
  branch: "cloud.card.lastAction.branch",
  manual: "cloud.card.lastAction.manual",
  undo: "cloud.card.lastAction.undo",
} as const satisfies Record<HeadReason, Key>;

export function cloudErrorKey(code: string): Key | null {
  return Object.hasOwn(ERROR_KEYS, code) ? ERROR_KEYS[code as keyof typeof ERROR_KEYS] : null;
}

export function lastActionKey(reason: HeadReason): Key {
  return ACTION_KEYS[reason];
}

export function cloudFactKey(view: CloudGameView, hasFolder: boolean): Key {
  if (!hasFolder) return "cloud.fact.setup";
  if (!view.enabled) return "cloud.fact.off";
  if (view.openElsewhere) return "cloud.card.openElsewhere";
  if (view.state && view.lastSnapshotAt !== null) {
    return view.state === "inCloud" ? "cloud.fact.inCloud" : "cloud.fact.inFolder";
  }
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
  "cloud.fact.inCloud": "cloud.panel.title.inCloud",
  "cloud.fact.inFolder": "cloud.panel.title.inFolder",
  "cloud.fact.notSynced": "cloud.panel.title.notSynced",
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

export function playtimeLabel(
  seconds: number | null,
): { key: "cloud.history.noHours" } | { key: "cloud.history.hours"; seconds: number } | null {
  if (seconds === null) return null;
  return seconds > 0 ? { key: "cloud.history.hours", seconds } : { key: "cloud.history.noHours" };
}
