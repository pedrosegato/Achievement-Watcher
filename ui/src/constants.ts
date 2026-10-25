export const EVENTS = {
  gamesChanged: "games-changed",
  settingsChanged: "settings-changed",
  scanProgress: "scan-progress",
  overlayShown: "overlay-shown",
  updateAvailable: "update-available",
  ffmpegProgress: "ffmpeg-progress",
} as const;

export const APP_NAME = "Achievement Watcher";
export const OVERLAY_HASH = "#overlay";
export const DEFAULT_HOTKEY = "Ctrl+Shift+A";
export const ROOT_ELEMENT_ID = "root";
export const OVERLAY_CLASS = "overlay";
export const PROTOCOL_ORIGIN = "http://aw.localhost";
export const DEFAULT_PRESET = "aw-minimal";
export const POPUP_MARGIN = 48;
export const SCREEN = { width: 1920, height: 1080 } as const;

export const SCAN_DONE = 100;

export const THEMES = ["system", "light", "dark"] as const;

export const POSITIONS = [
  "top-left",
  "top-center",
  "top-right",
  "middle-left",
  "middle-right",
  "bottom-left",
  "bottom-center",
  "bottom-right",
] as const;

export const AUTO_LANGUAGE = "auto";
export const FALLBACK_LOCALE = "en";

export const ACHIEVEMENT_LANGUAGES = [
  "english",
  "brazilian",
  "portuguese",
  "spanish",
  "latam",
  "french",
  "german",
  "italian",
  "russian",
  "polish",
  "turkish",
  "ukrainian",
  "japanese",
  "koreana",
  "schinese",
  "tchinese",
] as const;

export const SOUND_EVENTS = ["unlock", "rare", "platinum"] as const;
export const PREVIEW_KINDS = ["unlock", "rare", "platinum", "progress"] as const;
export const VIDEO_QUALITIES = ["light", "balanced", "high"] as const;
export const VIDEO_FORMATS = ["compatible", "small"] as const;
export const VIDEO_FPS = [30, 60, 120] as const;
export const ACHIEVEMENT_FILTERS = ["all", "unlocked", "locked"] as const;
export const USER_FOLDERS = ["presets", "sounds", "data"] as const;

export const RANGES = {
  scale: { min: 0.5, max: 2, step: 0.05 },
  volume: { min: 0, max: 1, step: 0.05 },
  rareThreshold: { min: 1, max: 50, step: 1 },
  silentMinutes: { min: 0, max: 1440, step: 1 },
  videoBefore: { min: 5, max: 60, step: 1 },
  videoAfter: { min: 0, max: 15, step: 1 },
} as const;

export const RARE_BADGE_PERCENT = 10;

export const TIMINGS = {
  statusFlashMs: 1500,
  rescanFeedbackMs: 1500,
  ffmpegPollMs: 1500,
  updateRecheckMs: 2500,
  soundFallbackMs: 1500,
} as const;

export const PLACEHOLDERS = {
  gamesFolder: "C:\\Games",
  ffmpeg: "C:\\ffmpeg\\bin\\ffmpeg.exe",
} as const;
