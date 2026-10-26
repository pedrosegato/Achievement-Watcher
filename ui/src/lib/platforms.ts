import type { ExternalId } from "../types/domain";

const SYNTHETIC_BASE = 0x80000000;
const LABELS: Record<string, string> = {
  steam: "Steam",
  uplay: "Ubisoft",
  epic: "Epic",
  gog: "GOG",
};

export function platformLabel(platform: string): string {
  return LABELS[platform] ?? platform;
}

export function isSynthetic(appId: number): boolean {
  return appId >= SYNTHETIC_BASE;
}

export function externalLabel(key: ExternalId): string {
  return `${platformLabel(key.platform)} ${key.id}`;
}
