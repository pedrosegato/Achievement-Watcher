interface KeyInput {
  key: string;
  code: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "OS", "AltGraph"]);

function keyName(code: string): string {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  return code;
}

export function comboFromKey(input: KeyInput): string | null {
  if (MODIFIER_KEYS.has(input.key) || !input.code) return null;
  const modifiers = [
    input.ctrlKey && "Ctrl",
    input.altKey && "Alt",
    input.shiftKey && "Shift",
    input.metaKey && "Super",
  ].filter((part): part is string => Boolean(part));
  if (modifiers.length === 0) return null;
  return [...modifiers, keyName(input.code)].join("+");
}

export function splitCombo(combo: string): string[] {
  return combo
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean);
}
