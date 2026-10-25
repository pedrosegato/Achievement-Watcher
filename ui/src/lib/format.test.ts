import { describe, expect, it } from "vitest";
import { formatDateTime, formatDay, formatHours, formatPercent } from "./format";

const SAMPLE = 1775507489;

describe("formatPercent", () => {
  it("uses the locale decimal and drops trailing zeros", () => {
    expect(formatPercent(7.2, "pt-BR")).toBe("7,2%");
    expect(formatPercent(7.2, "en")).toBe("7.2%");
    expect(formatPercent(28, "pt-BR")).toBe("28%");
  });

  it("keeps two decimals below one percent", () => {
    expect(formatPercent(0.04, "pt-BR")).toBe("0,04%");
  });
});

describe("formatHours", () => {
  it("shows whole hours from one hundred up", () => {
    expect(formatHours(28200 * 60, "pt-BR")).toBe("470 h");
  });

  it("shows one decimal below one hundred hours", () => {
    expect(formatHours(3063 * 60, "pt-BR")).toBe("51,0 h");
    expect(formatHours(3063 * 60, "en")).toBe("51.0 h");
  });

  it("shows minutes below one hour", () => {
    expect(formatHours(45 * 60, "pt-BR")).toBe("45 min");
  });
});

describe("dates", () => {
  it("formats date and time like the spec", () => {
    expect(formatDateTime(SAMPLE, "pt-BR")).toBe("6 abr 2026, 20:31");
    expect(formatDateTime(SAMPLE, "en")).toBe("Apr 6, 2026, 8:31 PM");
  });

  it("formats a day without the time", () => {
    expect(formatDay(SAMPLE, "pt-BR")).toBe("6 abr 2026");
    expect(formatDay(SAMPLE, "en")).toBe("Apr 6, 2026");
  });
});
