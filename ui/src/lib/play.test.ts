import { describe, expect, it } from "vitest";
import type { PlayState } from "../types/domain";
import { closingView, launchLine, playButtonView, playErrorKey, playMoment } from "./play";

const view = (state: PlayState) => playButtonView(state);

describe("playButtonView", () => {
  it("offers to play when idle", () => {
    expect(view({ type: "idle", needsExe: false })).toEqual({
      labelKey: "play.play",
      busy: false,
      disabled: false,
      action: "play",
      stuck: false,
      phase: "ready",
    });
  });

  it("asks for the exe when the game is not installed here", () => {
    expect(view({ type: "idle", needsExe: true })).toMatchObject({
      labelKey: "play.pickExe",
      action: "pickExe",
      disabled: false,
    });
  });

  it("is busy while syncing and passes the stuck flag", () => {
    expect(view({ type: "syncing", stuck: false })).toEqual({
      labelKey: "play.syncing",
      busy: true,
      disabled: true,
      action: "none",
      stuck: false,
      phase: "pulling",
    });
    expect(view({ type: "syncing", stuck: true }).stuck).toBe(true);
  });

  it("is busy while the game opens", () => {
    expect(view({ type: "launching" })).toMatchObject({
      labelKey: "play.launching",
      busy: true,
      disabled: true,
      action: "none",
    });
  });

  it("turns into stop while the game runs", () => {
    expect(view({ type: "running" })).toMatchObject({
      labelKey: "play.stop",
      busy: false,
      disabled: false,
      action: "stop",
    });
  });

  it("syncs again after closing", () => {
    expect(view({ type: "exiting" })).toMatchObject({
      labelKey: "play.syncing",
      busy: true,
      disabled: true,
      action: "none",
    });
  });

  it("lets the player try again after a failed launch", () => {
    expect(view({ type: "failed", reason: "launchUnconfirmed" })).toMatchObject({
      labelKey: "play.play",
      disabled: false,
      action: "play",
    });
  });

  it("names a motion phase for every state", () => {
    expect(view({ type: "idle", needsExe: true }).phase).toBe("pickExe");
    expect(view({ type: "launching" }).phase).toBe("launching");
    expect(view({ type: "running" }).phase).toBe("running");
    expect(view({ type: "exiting" }).phase).toBe("pushing");
    expect(view({ type: "failed", reason: "launchFailed" }).phase).toBe("ready");
  });

  it("stays ready to play while the open elsewhere warning is shown", () => {
    expect(view({ type: "openElsewhere", device: "Notebook", since: 10 })).toMatchObject({
      labelKey: "play.play",
      busy: false,
      disabled: false,
      action: "play",
    });
  });
});

describe("launchLine", () => {
  it("names how the game will open", () => {
    expect(launchLine({ exe: null, chosen: false, steam: true })).toEqual({
      key: "play.menu.steam",
      name: "",
    });
    expect(
      launchLine({
        exe: String.raw`C:\Games\ELDEN RING\Game\ersc_launcher.exe`,
        chosen: true,
        steam: false,
      }),
    ).toEqual({ key: "play.menu.exe", name: "ersc_launcher.exe" });
    expect(launchLine({ exe: null, chosen: false, steam: false })).toEqual({
      key: "play.menu.none",
      name: "",
    });
  });
});

describe("playMoment", () => {
  it("rumbles when the game starts running", () => {
    expect(playMoment("launching", "running")).toBe("rumble");
    expect(playMoment("pulling", "running")).toBe("rumble");
  });

  it("celebrates only a finished upload after closing", () => {
    expect(playMoment("pushing", "ready")).toBe("synced");
    expect(playMoment("pulling", "ready")).toBeNull();
    expect(playMoment("running", "pushing")).toBeNull();
  });

  it("stays quiet on the first render and on repeats", () => {
    expect(playMoment(undefined, "running")).toBeNull();
    expect(playMoment("running", "running")).toBeNull();
  });
});

describe("closingView", () => {
  it("is a busy closing step right after stop is confirmed", () => {
    expect(closingView()).toEqual({
      labelKey: "play.closing",
      busy: true,
      disabled: true,
      action: "none",
      stuck: false,
      phase: "closing",
    });
  });

  it("does not rumble or celebrate around closing", () => {
    expect(playMoment("running", "closing")).toBeNull();
    expect(playMoment("closing", "pushing")).toBeNull();
  });
});

describe("playErrorKey", () => {
  it("names the known failures", () => {
    expect(playErrorKey("launchUnconfirmed", "play.failed.generic")).toBe(
      "play.failed.launchUnconfirmed",
    );
    expect(playErrorKey("notAnExe", "play.failed.generic")).toBe("play.error.notAnExe");
  });

  it("falls back to the action that failed instead of blaming the launch", () => {
    expect(playErrorKey("Access is denied. (os error 5)", "play.failed.stopFailed")).toBe(
      "play.failed.stopFailed",
    );
    expect(playErrorKey("boom", "play.failed.generic")).toBe("play.failed.generic");
  });
});
