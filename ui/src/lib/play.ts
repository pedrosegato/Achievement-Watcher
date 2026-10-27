import type { Key } from "../i18n/dictionary";
import type { LaunchChoice, PlayState } from "../types/domain";

export type PlayAction = "play" | "stop" | "pickExe" | "none";

export interface PlayButtonView {
  labelKey: Key;
  busy: boolean;
  disabled: boolean;
  action: PlayAction;
  stuck: boolean;
}

const BUSY = { busy: true, disabled: true, action: "none", stuck: false } as const;
const READY = { busy: false, disabled: false, stuck: false } as const;

export function playButtonView(state: PlayState): PlayButtonView {
  switch (state.type) {
    case "idle":
      return state.needsExe
        ? { ...READY, labelKey: "play.pickExe", action: "pickExe" }
        : { ...READY, labelKey: "play.play", action: "play" };
    case "syncing":
      return { ...BUSY, labelKey: "play.syncing", stuck: state.stuck };
    case "launching":
      return { ...BUSY, labelKey: "play.launching" };
    case "running":
      return { ...READY, labelKey: "play.stop", action: "stop" };
    case "exiting":
      return { ...BUSY, labelKey: "play.syncing" };
    case "failed":
    case "openElsewhere":
      return { ...READY, labelKey: "play.play", action: "play" };
  }
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export function launchLine(choice: LaunchChoice): { key: Key; name: string } {
  if (choice.exe) return { key: "play.menu.exe", name: fileName(choice.exe) };
  if (choice.steam) return { key: "play.menu.steam", name: "" };
  return { key: "play.menu.none", name: "" };
}
