import { describe, expect, it, vi } from "vitest";
import { motionMs, parseDuration } from "./motion";

describe("parseDuration", () => {
  it("reads milliseconds and seconds", () => {
    expect(parseDuration("120ms")).toBe(120);
    expect(parseDuration(" 3.2s ")).toBe(3200);
  });

  it("falls back to zero", () => {
    expect(parseDuration("")).toBe(0);
    expect(parseDuration("fast")).toBe(0);
  });
});

describe("motionMs", () => {
  it("reads each token from the page only once", () => {
    const read = vi.fn(() => "120ms");
    expect(motionMs("--test-delay", read)).toBe(120);
    expect(motionMs("--test-delay", read)).toBe(120);
    expect(read).toHaveBeenCalledTimes(1);
  });
});
