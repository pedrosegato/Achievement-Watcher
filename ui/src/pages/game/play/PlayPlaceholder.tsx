import { MAIN } from "./styles";

export function PlayPlaceholder() {
  return (
    <div aria-hidden className="flex gap-px">
      <div className={`${MAIN} rounded-card bg-surface-2`} />
      <div className="h-11 w-9 rounded-r-card bg-surface-2" />
    </div>
  );
}
