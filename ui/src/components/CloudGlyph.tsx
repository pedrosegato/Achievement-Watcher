import { useChanged } from "../hooks/useChanged";
import type { CloudMood } from "../lib/cloud";
import { cn } from "../lib/cn";

export type GlyphMood = CloudMood | "pushing";

const BODY = "M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z";

const ENTRANCE: Partial<Record<GlyphMood, string>> = {
  synced: "cloud-hop",
  failed: "cloud-shake",
};

function Marks({ mood, fresh }: { mood: GlyphMood; fresh: boolean }) {
  switch (mood) {
    case "syncing":
      return (
        <>
          <path className="arrow-up" d="M10.5 16.5V12.5M9 14l1.5-1.5L12 14" />
          <path className="arrow-down" d="M14.5 12.5v4M13 15l1.5 1.5L16 15" />
        </>
      );
    case "pulling":
      return <path className="drop-down" d="M12.5 11.5v5M10.5 14.5l2 2 2-2" />;
    case "pushing":
      return <path className="drop-up" d="M12.5 16.5v-5M10.5 13.5l2-2 2 2" />;
    case "synced":
      return (
        <path
          className={fresh ? "check-draw" : undefined}
          pathLength={1}
          d="M9.75 14.25l2 2 3.5-3.75"
        />
      );
    case "failed":
      return (
        <>
          <path d="M12.5 11.25v3.25" />
          <circle cx="12.5" cy="16.6" r="0.4" fill="currentColor" />
        </>
      );
    case "playing":
      return (
        <>
          <path className="doze" d="M16.5 3.5h2.5l-2.5 2.5h2.5" />
          <path className="doze doze-late" d="M19.5 1h1.75l-1.75 1.75h1.75" />
        </>
      );
    case "off":
      return <path d="M4 4l16 16" />;
    case "idle":
      return null;
  }
}

export function CloudGlyph({
  mood,
  size = 22,
  entrance = false,
  className,
}: {
  mood: GlyphMood;
  size?: number;
  entrance?: boolean;
  className?: string;
}) {
  const changed = useChanged(mood);
  const fresh = changed || entrance;

  return (
    <svg
      key={mood}
      aria-hidden
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={cn("cloud-glyph flex-none", fresh && ENTRANCE[mood], className)}
    >
      <path d={BODY} />
      <g className={fresh ? "glyph-in" : undefined}>
        <Marks mood={mood} fresh={fresh} />
      </g>
    </svg>
  );
}
