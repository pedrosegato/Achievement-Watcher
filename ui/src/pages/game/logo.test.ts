import { describe, expect, it } from "vitest";
import { heroFallback, logoBox, logoObjectPosition } from "./logo";

describe("logoBox", () => {
  it("defaults to bottom left at 40 by 50", () => {
    expect(logoBox(null)).toEqual({ "--w": 40, "--h": 50, left: "3.5%", bottom: "11%" });
  });

  it("places each Steam anchor", () => {
    const size = { widthPct: 77.24, heightPct: 94.68 };
    expect(logoBox({ pin: "bottom-left", ...size })).toEqual({
      "--w": 77.24,
      "--h": 94.68,
      left: "3.5%",
      bottom: "11%",
    });
    expect(logoBox({ pin: "bottom-center", ...size })).toMatchObject({
      left: "50%",
      bottom: "11%",
      translate: "-50% 0",
    });
    expect(logoBox({ pin: "center-center", ...size })).toMatchObject({
      left: "50%",
      top: "55%",
      translate: "-50% -50%",
    });
    expect(logoBox({ pin: "upper-center", ...size })).toMatchObject({
      left: "50%",
      top: "22%",
      translate: "-50% 0",
    });
    expect(logoBox({ pin: "upper-left", ...size })).toMatchObject({ left: "3.5%", top: "22%" });
  });

  it("aligns the image inside the box with the anchor", () => {
    expect(logoObjectPosition(null)).toBe("left bottom");
    expect(logoObjectPosition({ pin: "center-center", widthPct: 1, heightPct: 1 })).toBe("center");
    expect(logoObjectPosition({ pin: "upper-left", widthPct: 1, heightPct: 1 })).toBe("left top");
  });
});

describe("heroFallback", () => {
  it("prefers the hero, then a blurred header, then a plain block", () => {
    expect(heroFallback(true, true)).toBe("hero");
    expect(heroFallback(false, true)).toBe("header-blur");
    expect(heroFallback(false, false)).toBe("plain");
  });
});
