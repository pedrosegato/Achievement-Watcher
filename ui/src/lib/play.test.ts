import { describe, expect, it } from "vitest";
import type { PlayState } from "../types/domain";
import { launchLine, playButtonView } from "./play";

const view = (state: PlayState) => playButtonView(state);

describe("playButtonView", () => {
  it("offers to play when idle", () => {
    expect(view({ type: "idle", needsExe: false })).toEqual({
      labelKey: "play.play",
      busy: false,
      disabled: false,
      action: "play",
      stuck: false,
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

  it("shows syncing while the save is sent after closing", () => {
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
