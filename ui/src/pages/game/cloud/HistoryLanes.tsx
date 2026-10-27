import type { GraphRow } from "../../../lib/graph";
import { DOT, LANE, laneX, ROW } from "./history";

export function HistoryLanes({
  row,
  lanes,
  principal,
}: {
  row: GraphRow;
  lanes: number;
  principal: boolean;
}) {
  const width = lanes * LANE;
  const x = laneX(row.lane);
  const middle = ROW / 2;

  return (
    <svg
      width={width}
      height={ROW}
      viewBox={`0 0 ${width} ${ROW}`}
      className="flex-none text-dim/60"
      aria-hidden="true"
    >
      {row.lanesAbove.map((lane) => (
        <line
          key={`pass-${lane}`}
          x1={laneX(lane)}
          x2={laneX(lane)}
          y1={0}
          y2={ROW}
          stroke="currentColor"
          strokeWidth={2}
        />
      ))}
      {row.merges.map((lane) => (
        <path
          key={`merge-${lane}`}
          d={`M ${laneX(lane)} 0 C ${laneX(lane)} ${middle * 0.6}, ${x} ${middle * 0.4}, ${x} ${middle}`}
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
        />
      ))}
      {row.incoming && (
        <line x1={x} x2={x} y1={0} y2={middle} stroke="currentColor" strokeWidth={2} />
      )}
      {row.continues && (
        <line x1={x} x2={x} y1={middle} y2={ROW} stroke="currentColor" strokeWidth={2} />
      )}
      <circle
        cx={x}
        cy={middle}
        r={principal ? DOT + 1 : DOT}
        className={principal ? "fill-text stroke-raised" : "fill-raised stroke-dim"}
        strokeWidth={2}
      />
    </svg>
  );
}
