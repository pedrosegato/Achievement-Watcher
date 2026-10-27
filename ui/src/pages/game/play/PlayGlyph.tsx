import { Icon } from "../../../components/Icon";
import type { PlayPhase } from "../../../lib/play";
import { SyncRoute } from "./SyncRoute";

export function PlayGlyph({ phase }: { phase: PlayPhase }) {
  switch (phase) {
    case "ready":
      return <Icon name="play" size={18} className="play-nudge" />;
    case "pickExe":
      return <Icon name="folder-open" size={18} />;
    case "pulling":
      return <SyncRoute direction="down" />;
    case "pushing":
      return <SyncRoute direction="up" />;
    case "launching":
      return (
        <span className="play-dash">
          <Icon name="play" size={18} />
        </span>
      );
    case "running":
      return (
        <span className="stop-x">
          <Icon name="close" size={18} />
        </span>
      );
  }
}
