import type { VideoFormat, VideoQuality } from "../types/domain";

const MBPS_AT_30: Record<VideoQuality, number> = { light: 3, balanced: 6, high: 8 };
const HIGH_FPS_COST = 1.6;
const HIGHEST_FPS_COST = 2.4;
const FORMAT_COST: Record<VideoFormat, number> = { compatible: 1.4, small: 1 };
const BITS_PER_BYTE = 8;

function fpsCost(fps: number): number {
  if (fps > 60) return HIGHEST_FPS_COST;
  return fps > 30 ? HIGH_FPS_COST : 1;
}

export function clipSizeMb(
  seconds: number,
  fps: number,
  quality: VideoQuality,
  format: VideoFormat,
): number {
  const mbps = MBPS_AT_30[quality] * fpsCost(fps) * FORMAT_COST[format];
  return Math.max(1, Math.round((mbps * seconds) / BITS_PER_BYTE));
}
