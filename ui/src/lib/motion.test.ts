import { describe, expect, it } from "vitest";
import { parseDuration } from "./motion";

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
