import type {
  AchievementFilter,
  Position,
  PreviewKind,
  SoundEvent,
  VideoFormat,
  VideoQuality,
} from "../types/domain";
import type { Key } from "./dictionary";

export const POSITION_LABELS: Record<Position, Key> = {
  "top-left": "settings.position.top-left",
  "top-center": "settings.position.top-center",
  "top-right": "settings.position.top-right",
  "middle-left": "settings.position.middle-left",
  "middle-right": "settings.position.middle-right",
  "bottom-left": "settings.position.bottom-left",
  "bottom-center": "settings.position.bottom-center",
  "bottom-right": "settings.position.bottom-right",
};

export const FILTER_LABELS: Record<AchievementFilter, Key> = {
  all: "game.all",
  unlocked: "game.unlocked",
  locked: "game.locked",
};

export const SOUND_EVENT_LABELS: Record<SoundEvent, Key> = {
  unlock: "settings.previewUnlock",
  rare: "settings.previewRare",
  platinum: "settings.previewPlatinum",
};

export const PREVIEW_LABELS: Record<PreviewKind, Key> = {
  ...SOUND_EVENT_LABELS,
  progress: "settings.previewProgress",
};

export const QUALITY_LABELS: Record<VideoQuality, Key> = {
  light: "settings.quality.light",
  balanced: "settings.quality.balanced",
  high: "settings.quality.high",
};

export const FORMAT_LABELS: Record<VideoFormat, Key> = {
  compatible: "settings.format.compatible",
  small: "settings.format.small",
};

export const FORMAT_HINTS: Record<VideoFormat, Key> = {
  compatible: "settings.format.compatibleHint",
  small: "settings.format.smallHint",
};

export const QUALITY_HINTS: Record<VideoQuality, Key> = {
  light: "settings.quality.lightHint",
  balanced: "settings.quality.balancedHint",
  high: "settings.quality.highHint",
};
