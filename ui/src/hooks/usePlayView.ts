import { useState } from "react";
import { closingView, type PlayButtonView, playButtonView } from "../lib/play";
import type { PlayState } from "../types/domain";

export function usePlayView(state: PlayState | undefined): {
  view: PlayButtonView | null;
  markClosing: (closing: boolean) => void;
} {
  const [closing, setClosing] = useState(false);
  const running = state?.type === "running";
  if (closing && state && !running) setClosing(false);
  if (!state) return { view: null, markClosing: setClosing };
  return {
    view: closing && running ? closingView() : playButtonView(state),
    markClosing: setClosing,
  };
}
