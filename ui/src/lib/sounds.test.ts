import { describe, expect, it } from "vitest";
import type { SoundPack } from "../types/domain";
import { packName } from "./sounds";

function pack(partial: Partial<SoundPack>): SoundPack {
  return {
    id: "achievement-watcher",
    name: "Achievement Watcher",
    builtin: true,
    hasRare: true,
    hasPlatinum: true,
    ...partial,
  };
}

const t = (key: string, vars?: Record<string, string | number>) =>
  vars ? `t:${key}:${Object.values(vars).join(",")}` : `t:${key}`;

describe("packName", () => {
  it("translates the built-in default pack", () => {
    expect(packName(pack({}), t)).toBe("t:settings.defaultPack");
  });

  it("keeps the name of other built-in packs", () => {
    expect(packName(pack({ id: "xbox", name: "Xbox" }), t)).toBe("Xbox");
  });

  it("marks a user pack that replaces the default as the user's own", () => {
    expect(packName(pack({ builtin: false, name: "Mine" }), t)).toBe("t:settings.customPack:Mine");
  });
});
