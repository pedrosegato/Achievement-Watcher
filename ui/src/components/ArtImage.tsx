import { type ReactNode, useState } from "react";
import { cn } from "../lib/cn";

interface ArtImageProps {
  src: string;
  className?: string;
  alt?: string;
  position?: string;
  fallback?: ReactNode;
  onMissing?: () => void;
}

export function ArtImage({
  src,
  className,
  alt = "",
  position,
  fallback = null,
  onMissing,
}: ArtImageProps) {
  const [failed, setFailed] = useState<string | null>(null);
  if (failed === src) return fallback;
  return (
    <img
      src={src}
      alt={alt}
      draggable={false}
      onError={() => {
        setFailed(src);
        onMissing?.();
      }}
      style={position ? { objectPosition: position } : undefined}
      className={cn("block size-full object-cover", className)}
    />
  );
}
