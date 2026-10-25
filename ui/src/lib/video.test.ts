import { describe, expect, it } from "vitest";
import { clipSizeMb } from "./video";

describe("clipSizeMb", () => {
  it("grows with length, frame rate and quality", () => {
    expect(clipSizeMb(15, 30, "balanced")).toBe(15);
    expect(clipSizeMb(15, 60, "balanced")).toBe(24);
    expect(clipSizeMb(75, 60, "high")).toBe(240);
  });

  it("never reports less than 1 MB", () => {
    expect(clipSizeMb(5, 30, "light")).toBe(3);
    expect(clipSizeMb(0, 30, "light")).toBe(1);
  });
});
