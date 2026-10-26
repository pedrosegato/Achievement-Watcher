const PLACEHOLDERS = ["a", "b", "c", "d", "e", "f", "g", "h"];

export function LibrarySkeleton() {
  return (
    <div className="library-grid" aria-hidden="true">
      {PLACEHOLDERS.map((key) => (
        <div key={key} className="rounded-card border border-line bg-surface">
          <div className="aspect-460/215 rounded-t-[3px] bg-surface-2" />
          <div className="flex flex-col gap-2.5 p-[13px]">
            <div className="h-4 w-2/3 rounded-tag bg-surface-2" />
            <div className="h-6 w-1/3 rounded-tag bg-surface-2" />
            <div className="h-[30px] w-full rounded-tag bg-surface-2" />
          </div>
        </div>
      ))}
    </div>
  );
}
