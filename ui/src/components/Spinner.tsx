import { cn } from "../lib/cn";

export function Spinner({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "spinning inline-block size-4 flex-none rounded-full border-2 border-current border-r-transparent",
        className,
      )}
    />
  );
}
