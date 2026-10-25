import { useState } from "react";

function attempt<T>(run: () => T, fallback: T): T {
  try {
    return run();
  } catch {
    return fallback;
  }
}

export function useStoredChoice<T extends string>(
  key: string,
  choices: readonly T[],
  fallback: T,
): [T, (value: T) => void] {
  const [value, setValue] = useState<T>(() => {
    const stored = attempt(() => window.localStorage.getItem(key), null);
    return choices.find((choice) => choice === stored) ?? fallback;
  });
  const update = (next: T) => {
    setValue(next);
    attempt(() => window.localStorage.setItem(key, next), undefined);
  };
  return [value, update];
}
