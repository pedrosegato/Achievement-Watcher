export function GameSkeleton() {
  return (
    <div aria-hidden="true">
      <div className="game-hero bg-surface-2" />
      <div className="flex gap-8 border-line border-b px-5 py-4">
        <div className="h-9 w-32 rounded-tag bg-surface-2" />
        <div className="h-9 w-40 rounded-tag bg-surface-2" />
        <div className="h-9 w-28 rounded-tag bg-surface-2" />
      </div>
      <div className="game-cols mx-auto max-w-[1440px] px-5 pt-5">
        <div className="h-64 rounded-card bg-surface-2" />
        <div className="h-64 rounded-card bg-surface-2" />
      </div>
    </div>
  );
}
