import { describe, expect, it } from "vitest";
import { cloudChangedKeys, playChangedKeys } from "./events";

describe("cloudChangedKeys", () => {
  it("refreshes only the game the event is about", () => {
    expect(cloudChangedKeys(7)).toEqual([["cloud-game", 7]]);
  });

  it("refreshes the overview and every game on a global change", () => {
    expect(cloudChangedKeys(null)).toEqual([["cloud-overview"], ["cloud-game"]]);
  });
});

describe("playChangedKeys", () => {
  it("refreshes the button state and the launch menu of that game", () => {
    expect(playChangedKeys(7)).toEqual([
      ["play-state", 7],
      ["launch-choice", 7],
    ]);
  });
});
