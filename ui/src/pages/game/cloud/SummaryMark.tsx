import { Icon } from "../../../components/Icon";
import { Spinner } from "../../../components/Spinner";
import type { CloudGameView } from "../../../types/domain";

export function SummaryMark({ view }: { view: CloudGameView }) {
  const status = view.status.type;
  if (status === "syncing") {
    return (
      <span className="grid size-9 flex-none place-items-center rounded-full bg-surface-2 text-text">
        <Spinner />
      </span>
    );
  }
  const synced = status === "sent" || status === "received";
  const tone = synced || status === "failed" ? "text-text" : "text-dim";
  const icon = status === "failed" ? "info" : synced ? "check" : "cloud";
  return (
    <span className={`grid size-9 flex-none place-items-center rounded-full bg-surface-2 ${tone}`}>
      <Icon name={icon} size={18} />
    </span>
  );
}
