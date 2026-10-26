import { describe, expect, it } from "vitest";
import { clipSizeMb } from "./video";

describe("clipSizeMb", () => {
  it("lands between the measured High 60 fps Compatible clips", () => {
    expect(clipSizeMb(20.5, 60, "high", "compatible")).toBe(69);
  });

  it("grows with length, frame rate and quality", () => {
    expect(clipSizeMb(15, 30, "balanced", "small")).toBe(17);
    expect(clipSizeMb(15, 60, "balanced", "small")).toBe(27);
    expect(clipSizeMb(75, 60, "high", "small")).toBe(180);
  });

  it("costs more at 120 fps", () => {
    expect(clipSizeMb(20, 120, "balanced", "small")).toBe(54);
  });

  it("expects bigger files from the compatible format", () => {
    expect(clipSizeMb(15, 60, "balanced", "compatible")).toBe(38);
  });

  it("never reports less than 1 MB", () => {
    expect(clipSizeMb(5, 30, "light", "small")).toBe(3);
    expect(clipSizeMb(0, 30, "light", "small")).toBe(1);
  });
});
