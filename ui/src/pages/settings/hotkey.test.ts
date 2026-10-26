import { describe, expect, it } from "vitest";
import { comboFromKey, splitCombo } from "./hotkey";

const key = (partial: Partial<Parameters<typeof comboFromKey>[0]>) => ({
  key: "",
  code: "",
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ...partial,
});

describe("comboFromKey", () => {
  it("builds the Tauri shortcut text", () => {
    expect(comboFromKey(key({ key: "a", code: "KeyA", ctrlKey: true, shiftKey: true }))).toBe(
      "Ctrl+Shift+A",
    );
    expect(comboFromKey(key({ key: "F9", code: "F9", altKey: true }))).toBe("Alt+F9");
    expect(comboFromKey(key({ key: "1", code: "Digit1", ctrlKey: true }))).toBe("Ctrl+1");
  });

  it("needs a modifier and a real key", () => {
    expect(comboFromKey(key({ key: "Shift", code: "ShiftLeft", shiftKey: true }))).toBeNull();
    expect(comboFromKey(key({ key: "a", code: "KeyA" }))).toBeNull();
  });
});

describe("splitCombo", () => {
  it("splits keys for display", () => {
    expect(splitCombo("Ctrl+Shift+A")).toEqual(["Ctrl", "Shift", "A"]);
  });
});
