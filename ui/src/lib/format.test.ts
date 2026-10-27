import { afterEach, describe, expect, it, vi } from "vitest";
import {
  formatBytes,
  formatDateTime,
  formatDay,
  formatHours,
  formatPercent,
  formatWhen,
} from "./format";

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

describe("formatter reuse", () => {
  afterEach(() => vi.restoreAllMocks());

  it("builds no new Intl formatter once a locale is warm", () => {
    formatDateTime(SAMPLE, "pt-BR");
    formatPercent(7.2, "pt-BR");
    formatHours(36_000, "pt-BR");
    const dates = vi.spyOn(Intl, "DateTimeFormat");
    const numbers = vi.spyOn(Intl, "NumberFormat");
    for (let i = 0; i < 50; i += 1) {
      formatDateTime(SAMPLE + i * 60, "pt-BR");
      formatPercent(i / 3, "pt-BR");
      formatHours(36_000 + i, "pt-BR");
    }
    expect(dates).not.toHaveBeenCalled();
    expect(numbers).not.toHaveBeenCalled();
  });
});

describe("formatBytes", () => {
  it("uses binary units and the locale decimal", () => {
    expect(formatBytes(52428800, "pt-BR")).toBe("50 MB");
    expect(formatBytes(1536, "en")).toBe("1.5 KB");
    expect(formatBytes(1536, "pt-BR")).toBe("1,5 KB");
    expect(formatBytes(512, "en")).toBe("512 B");
    expect(formatBytes(3 * 1024 ** 3, "en")).toBe("3 GB");
  });
});

describe("formatWhen", () => {
  it("says today and yesterday by name", () => {
    expect(formatWhen(SAMPLE, "pt-BR", SAMPLE + 600)).toBe("Hoje, 20:31");
    expect(formatWhen(SAMPLE, "en", SAMPLE + 600)).toBe("Today, 8:31 PM");
    expect(formatWhen(SAMPLE, "pt-BR", SAMPLE + 86_400)).toBe("Ontem, 20:31");
  });

  it("falls back to the full date for older moments", () => {
    expect(formatWhen(SAMPLE, "pt-BR", SAMPLE + 5 * 86_400)).toBe("6 abr 2026, 20:31");
  });
});
