import { describe, expect, it } from "vitest";
import { clipSizeMb } from "./video";

describe("clipSizeMb", () => {
  it("grows with length, frame rate and quality", () => {
    expect(clipSizeMb(15, 30, "balanced", "small")).toBe(11);
    expect(clipSizeMb(15, 60, "balanced", "small")).toBe(18);
    expect(clipSizeMb(75, 60, "high", "small")).toBe(120);
  });

  it("costs more at 120 fps", () => {
    expect(clipSizeMb(15, 120, "balanced", "small")).toBe(27);
  });

  it("expects bigger files from the compatible format", () => {
    expect(clipSizeMb(15, 60, "balanced", "compatible")).toBe(25);
  });

  it("never reports less than 1 MB", () => {
    expect(clipSizeMb(5, 30, "light", "small")).toBe(2);
    expect(clipSizeMb(0, 30, "light", "small")).toBe(1);
  });
});
