import type { VideoQuality } from "../types/domain";

const MBPS_AT_30: Record<VideoQuality, number> = { light: 3, balanced: 6, high: 8 };
const HIGH_FPS_COST = 1.6;
const BITS_PER_BYTE = 8;

export function clipSizeMb(seconds: number, fps: number, quality: VideoQuality): number {
  const mbps = MBPS_AT_30[quality] * (fps > 30 ? HIGH_FPS_COST : 1);
  return Math.max(1, Math.round((mbps * seconds) / BITS_PER_BYTE));
}
