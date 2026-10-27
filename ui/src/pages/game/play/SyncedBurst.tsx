import { CloudGlyph } from "../../../components/CloudGlyph";

const RAYS = [0, 45, 90, 135, 180, 225, 270, 315];
const PACKETS = [0, 1, 2];

export function SyncedBurst() {
  return (
    <span aria-hidden className="synced-burst relative inline-grid size-[26px] place-items-center">
      {PACKETS.map((packet) => (
        <span
          key={packet}
          className="burst-packet"
          style={{ left: `${9 + packet * 4}px`, animationDelay: `${packet * 70}ms` }}
        />
      ))}
      <CloudGlyph mood="synced" size={24} entrance />
      <svg
        aria-hidden
        width={44}
        height={44}
        viewBox="-22 -22 44 44"
        stroke="currentColor"
        strokeWidth={1.8}
        strokeLinecap="round"
        className="burst-rays pointer-events-none absolute overflow-visible"
      >
        {RAYS.map((angle) => (
          <line key={angle} x1={0} y1={-14} x2={0} y2={-18.5} transform={`rotate(${angle})`} />
        ))}
      </svg>
    </span>
  );
}
