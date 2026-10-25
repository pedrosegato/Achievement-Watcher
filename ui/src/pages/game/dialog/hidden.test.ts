import { describe, expect, it } from "vitest";
import { hiddenReducer } from "./hidden";

describe("hiddenReducer", () => {
  it("reveals and hides single achievements", () => {
    let state = hiddenReducer(new Set(), { type: "reveal", apiName: "A" });
    state = hiddenReducer(state, { type: "reveal", apiName: "B" });
    state = hiddenReducer(state, { type: "hide", apiName: "A" });
    expect([...state]).toEqual(["B"]);
  });

  it("revealResetsOnClose", () => {
    const revealed = hiddenReducer(new Set(), { type: "reveal", apiName: "A" });
    expect(hiddenReducer(revealed, { type: "reset" }).size).toBe(0);
    const empty: ReadonlySet<string> = new Set();
    expect(hiddenReducer(empty, { type: "reset" })).toBe(empty);
  });
});
