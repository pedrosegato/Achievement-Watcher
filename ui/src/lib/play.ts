import type { Key } from "../i18n/dictionary";
import type { LaunchChoice, PlayState } from "../types/domain";

export type PlayAction = "play" | "stop" | "pickExe" | "none";

export type PlayPhase =
  | "ready"
  | "pickExe"
  | "pulling"
  | "launching"
  | "running"
  | "closing"
  | "pushing";

export interface PlayButtonView {
  labelKey: Key;
  busy: boolean;
  disabled: boolean;
  action: PlayAction;
  stuck: boolean;
  phase: PlayPhase;
}

const BUSY = { busy: true, disabled: true, action: "none", stuck: false } as const;
const READY = { busy: false, disabled: false, stuck: false } as const;

export function playButtonView(state: PlayState): PlayButtonView {
  switch (state.type) {
    case "idle":
      return state.needsExe
        ? { ...READY, labelKey: "play.pickExe", action: "pickExe", phase: "pickExe" }
        : { ...READY, labelKey: "play.play", action: "play", phase: "ready" };
    case "syncing":
      return { ...BUSY, labelKey: "play.syncing", stuck: state.stuck, phase: "pulling" };
    case "launching":
      return { ...BUSY, labelKey: "play.launching", phase: "launching" };
    case "running":
      return { ...READY, labelKey: "play.stop", action: "stop", phase: "running" };
    case "exiting":
      return { ...BUSY, labelKey: "play.syncing", phase: "pushing" };
    case "failed":
    case "openElsewhere":
      return { ...READY, labelKey: "play.play", action: "play", phase: "ready" };
  }
}

export function closingView(): PlayButtonView {
  return { ...BUSY, labelKey: "play.closing", phase: "closing" };
}

const PLAY_ERRORS: Record<string, Key> = {
  launchUnconfirmed: "play.failed.launchUnconfirmed",
  launchFailed: "play.failed.launchFailed",
  notAnExe: "play.error.notAnExe",
};

export function playErrorKey(code: string, fallback: Key): Key {
  return Object.hasOwn(PLAY_ERRORS, code) ? (PLAY_ERRORS[code] as Key) : fallback;
}

export type PlayMoment = "rumble" | "synced";

export function playMoment(previous: PlayPhase | undefined, next: PlayPhase): PlayMoment | null {
  if (previous === undefined || previous === next) return null;
  if (next === "running") return "rumble";
  if (previous === "pushing" && next === "ready") return "synced";
  return null;
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export function launchLine(choice: LaunchChoice): { key: Key; name: string } {
  if (choice.exe) return { key: "play.menu.exe", name: fileName(choice.exe) };
  if (choice.steam) return { key: "play.menu.steam", name: "" };
  return { key: "play.menu.none", name: "" };
}
