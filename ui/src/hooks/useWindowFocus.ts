import { useEffect, useEffectEvent } from "react";

export function useWindowFocus(handler: () => void) {
  const onFocus = useEffectEvent(handler);

  useEffect(() => {
    const listener = () => onFocus();
    window.addEventListener("focus", listener);
    return () => window.removeEventListener("focus", listener);
  }, []);
}
