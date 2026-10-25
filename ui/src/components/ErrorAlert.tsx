import type { MessageProps } from "../types/ui";

export function ErrorAlert({ message }: MessageProps) {
  if (!message) return null;
  return (
    <p
      role="alert"
      className="rounded-card border border-line bg-surface px-3 py-2 text-[13px] text-muted"
    >
      {message}
    </p>
  );
}
