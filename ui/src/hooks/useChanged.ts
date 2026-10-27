import { useState } from "react";

export function useChanged<T>(value: T): boolean {
  const [previous, setPrevious] = useState(value);
  const [changed, setChanged] = useState(false);
  if (value !== previous) {
    setPrevious(value);
    setChanged(true);
  }
  return changed;
}
