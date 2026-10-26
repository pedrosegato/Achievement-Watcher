import { listen } from "@tauri-apps/api/event";
import { useEffect, useEffectEvent } from "react";

export function useTauriEvent<T = unknown>(name: string, handler: (payload: T) => void) {
  const onEvent = useEffectEvent(handler);

  useEffect(() => {
    const pending = listen<T>(name, (event) => onEvent(event.payload));
    return () => {
      pending.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [name]);
}
