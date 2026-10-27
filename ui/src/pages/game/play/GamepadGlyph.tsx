const BODY =
  "M15 4h18c5.2 0 8.5 3.6 9.6 9l2.2 12c.6 3.4-1.6 5.8-4.4 5.8-2 0-3.4-1.2-4.4-3L32.8 22H15.2l-3.2 5.8c-1 1.8-2.4 3-4.4 3-2.8 0-5-2.4-4.4-5.8l2.2-12C6.5 7.6 9.8 4 15 4Z";

const KEYS = [
  { cx: 35, cy: 15.6 },
  { cx: 38.2, cy: 12.4 },
  { cx: 31.8, cy: 12.4 },
  { cx: 35, cy: 9.2 },
];

export function GamepadGlyph({ off = false }: { off?: boolean }) {
  return (
    <span aria-hidden className={`inline-grid ${off ? "pad-off" : "pad-pop"}`}>
      <svg
        aria-hidden
        width={44}
        height={31}
        viewBox="0 0 48 34"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.6}
        strokeLinecap="round"
        strokeLinejoin="round"
        className={off ? "overflow-visible" : "pad-bob overflow-visible"}
      >
        <path d={BODY} fill="currentColor" fillOpacity={0.14} />
        <path className={off ? undefined : "pad-dpad"} d="M14 9v6.8M10.6 12.4h6.8" />
        {KEYS.map((key, index) => (
          <circle
            key={`${key.cx}-${key.cy}`}
            className={off ? "pad-key-off" : "pad-key"}
            cx={key.cx}
            cy={key.cy}
            r={1.1}
            fill="currentColor"
            stroke="none"
            style={{
              animationDelay: `${off ? (KEYS.length - 1 - index) * 170 + 120 : index * 150}ms`,
            }}
          />
        ))}
      </svg>
    </span>
  );
}
