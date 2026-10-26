import { describe, expect, it } from "vitest";
import en from "./locales/en.json";
import ptBR from "./locales/pt-BR.json";

const DECORATIVE = [0xb7, 0x2026, 0x2192, 0x2190, 0x2605].map((code) => String.fromCodePoint(code));
const LOCALES: Record<string, Record<string, string>> = { en, "pt-BR": ptBR };

describe("locales", () => {
  it("localesMatchKeys", () => {
    expect(Object.keys(ptBR).sort()).toEqual(Object.keys(en).sort());
  });

  it("noDecorativeUnicode", () => {
    const offenders = Object.entries(LOCALES).flatMap(([locale, entries]) =>
      Object.entries(entries)
        .filter(([, value]) => DECORATIVE.some((glyph) => value.includes(glyph)))
        .map(([key]) => `${locale}:${key}`),
    );
    expect(offenders).toEqual([]);
  });
});
