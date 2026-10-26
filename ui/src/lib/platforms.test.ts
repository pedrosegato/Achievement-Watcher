import { describe, expect, it } from "vitest";
import { externalLabel, isSynthetic, platformLabel } from "./platforms";

describe("platforms", () => {
  it("platformLabel names known platforms", () => {
    expect(platformLabel("uplay")).toBe("Ubisoft");
    expect(platformLabel("epic")).toBe("Epic");
    expect(platformLabel("gog")).toBe("GOG");
    expect(platformLabel("steam")).toBe("Steam");
    expect(platformLabel("other")).toBe("other");
  });

  it("isSynthetic flags ids from the high range", () => {
    expect(isSynthetic(0x80000000)).toBe(true);
    expect(isSynthetic(4294967295)).toBe(true);
    expect(isSynthetic(1245620)).toBe(false);
  });

  it("externalLabel joins platform and id", () => {
    expect(externalLabel({ platform: "epic", id: "Carnation" })).toBe("Epic Carnation");
  });
});
