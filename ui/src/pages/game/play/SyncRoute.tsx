const ARC_UP = "M14 6 Q27 -7 40 9";
const ARC_DOWN = "M40 9 Q27 -7 14 6";
const CLOUD = "M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z";
const PACKETS = [0, 1, 2];

export function SyncRoute({ direction }: { direction: "up" | "down" }) {
  const arc = direction === "up" ? ARC_UP : ARC_DOWN;

  return (
    <span aria-hidden className="relative inline-block h-6 w-[60px] flex-none">
      <svg
        aria-hidden
        width={60}
        height={24}
        viewBox="0 0 60 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.5}
        strokeLinecap="round"
        strokeLinejoin="round"
        className="overflow-visible"
      >
        <rect x={1} y={6} width={14} height={10} rx={2} />
        <path d="M5.5 20h6M8.5 16v4" />
        <path d={CLOUD} transform="translate(36 3.5) scale(0.95)" />
        <path className="route-arc" d={ARC_UP} strokeDasharray="1.5 3" strokeOpacity={0.45} />
      </svg>
      {PACKETS.map((packet) => (
        <span
          key={packet}
          className="route-packet"
          style={{ offsetPath: `path("${arc}")`, animationDelay: `${packet * 400}ms` }}
        />
      ))}
    </span>
  );
}
