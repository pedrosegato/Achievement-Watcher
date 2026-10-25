import { describe, expect, it } from "vitest";
import type { PresetInfo } from "../types/domain";
import { effectivePreset, popupFrame, presetName, previewUrl } from "./presets";

function preset(partial: Partial<PresetInfo>): PresetInfo {
  return {
    id: "aw-minimal",
    name: "Default",
    author: null,
    version: null,
    width: 520,
    height: 160,
    durationMs: 6950,
    builtin: true,
    valid: true,
    warning: null,
    bleed: [40, 40],
    ...partial,
  };
}

const t = (key: string) => `t:${key}`;

describe("presetName", () => {
  it("translates the built-in default style", () => {
    expect(presetName(preset({}), t)).toBe("t:settings.defaultStyle");
  });

  it("keeps the name of other built-in styles", () => {
    expect(presetName(preset({ id: "xbox", name: "Xbox One" }), t)).toBe("Xbox One");
  });

  it("keeps the name of a user style that replaces the default", () => {
    expect(presetName(preset({ builtin: false, name: "My minimal" }), t)).toBe("My minimal");
  });
});

describe("popupFrame", () => {
  it("places the window so the card sits at the screen margin", () => {
    expect(popupFrame(preset({}), "bottom-right", 1)).toEqual({
      x: 1392,
      y: 912,
      width: 520,
      height: 160,
    });
    expect(popupFrame(preset({}), "middle-left", 1)).toEqual({
      x: 8,
      y: 460,
      width: 520,
      height: 160,
    });
  });

  it("lets a scaled bleed hang past the screen edge", () => {
    expect(popupFrame(preset({}), "top-center", 1.5)).toEqual({
      x: 570,
      y: -12,
      width: 780,
      height: 240,
    });
  });
});

describe("previewUrl", () => {
  it("asks the style page for a looping preview of one kind and spot", () => {
    expect(previewUrl("xbox one", "rare", "top-left")).toBe(
      "http://aw.localhost/preset/xbox%20one/index.html?preview=loop&kind=rare&position=top-left",
    );
  });
});

describe("effectivePreset", () => {
  const list = [
    preset({}),
    preset({ id: "xbox", name: "Xbox One" }),
    preset({ id: "broken", valid: false }),
  ];

  it("finds the chosen style ignoring case", () => {
    expect(effectivePreset(list, "XBOX")?.id).toBe("xbox");
  });

  it("falls back to the default style like the popup does", () => {
    expect(effectivePreset(list, "neon")?.id).toBe("aw-minimal");
    expect(effectivePreset(list, "broken")?.id).toBe("aw-minimal");
  });
});
