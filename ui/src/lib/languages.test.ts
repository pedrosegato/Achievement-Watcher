import { describe, expect, it } from "vitest";
import { steamLanguageName } from "./languages";

describe("steamLanguageName", () => {
  it("names Steam languages in the app language", () => {
    expect(steamLanguageName("brazilian", "pt-BR")).toBe("português (Brasil)");
    expect(steamLanguageName("english", "en")).toBe("English");
  });

  it("falls back to the Steam id", () => {
    expect(steamLanguageName("klingon", "en")).toBe("klingon");
  });
});
