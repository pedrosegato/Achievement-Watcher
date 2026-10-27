export const LANE = 18;
export const ROW = 60;
export const DOT = 5;

export function laneX(lane: number): number {
  return lane * LANE + LANE / 2;
}
