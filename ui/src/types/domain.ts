import type {
  ACHIEVEMENT_FILTERS,
  POSITIONS,
  PREVIEW_KINDS,
  SOUND_EVENTS,
  THEMES,
  USER_FOLDERS,
  VIDEO_QUALITIES,
} from "../constants";

export type Position = (typeof POSITIONS)[number];
export type Theme = (typeof THEMES)[number];
export type Locale = string;
export type UiLanguage = string;

export interface LocaleInfo {
  code: Locale;
  name: string;
}
export type SoundEvent = (typeof SOUND_EVENTS)[number];
export type PreviewKind = (typeof PREVIEW_KINDS)[number];
export type VideoQuality = (typeof VIDEO_QUALITIES)[number];
export type AchievementFilter = (typeof ACHIEVEMENT_FILTERS)[number];
export type UserFolder = (typeof USER_FOLDERS)[number];

export interface GameRow {
  appId: number;
  name: string;
  unlocked: number;
  total: number;
  lastUnlockAt: number | null;
  coverUrl: string;
  playtimeSeconds: number;
  lastPlayed: number | null;
  playing: boolean;
  origin: string | null;
  originId: string | null;
  headerUrl: string;
  platinumAt: number | null;
  recent: RecentIcon[];
}

export interface RecentIcon {
  apiName: string;
  displayName: string;
  description: string;
  iconUrl: string | null;
  percent: number | null;
  unlockedAt: number;
  playtimeSeconds: number | null;
}

export type LogoPin =
  | "bottom-left"
  | "bottom-center"
  | "center-center"
  | "upper-center"
  | "upper-left";

export interface LogoPosition {
  pin: LogoPin;
  widthPct: number;
  heightPct: number;
}

export interface AchievementRow {
  apiName: string;
  displayName: string;
  description: string;
  iconUrl: string | null;
  iconGrayUrl: string | null;
  hidden: boolean;
  percent: number | null;
  unlockedAt: number | null;
  playtimeSeconds: number | null;
}

export interface GameDetail {
  appId: number;
  name: string;
  coverUrl: string;
  unlocked: number;
  total: number;
  playtimeSeconds: number;
  playing: boolean;
  origin: string | null;
  headerUrl: string;
  heroUrl: string;
  logoUrl: string;
  logoPosition: LogoPosition | null;
  platinumAt: number | null;
  achievements: AchievementRow[];
}

export interface Settings {
  preset: string;
  position: Position;
  scale: number;
  sound: boolean;
  soundVolume: number;
  soundId: string;
  rareThreshold: number;
  screenshot: boolean;
  screenshotDir: string | null;
  language: string | null;
  autostart: boolean;
  silentAfterMinutes: number;
  extraGameRoots: string[];
  extraGameDirs: string[];
  readSteamLibraries: boolean;
  steamAchievements: boolean;
  steamPopups: boolean;
  progressPopups: boolean;
  overlayHotkey: string;
  video: boolean;
  videoBefore: number;
  videoAfter: number;
  videoFps: number;
  videoQuality: VideoQuality;
  autoUpdate: boolean;
  ffmpegPath: string | null;
  videoDir: string | null;
  hideHiddenAchievements: boolean;
  uiLanguage: UiLanguage;
  theme: Theme;
}

export interface PresetInfo {
  id: string;
  name: string;
  author: string | null;
  version: string | null;
  width: number;
  height: number;
  durationMs: number;
  builtin: boolean;
  valid: boolean;
  warning: string | null;
  bleed: [number, number];
}

export interface SoundPack {
  id: string;
  name: string;
  builtin: boolean;
  hasRare: boolean;
  hasPlatinum: boolean;
}

export interface SourceRow {
  id: string;
  name: string;
  root: string | null;
  exists: boolean;
}

export interface FileReport {
  path: string;
  source_id: string;
  app_id: number;
  last_ok_at: number | null;
  last_error: string | null;
  last_count: number | null;
}

export interface Unresolved {
  source_id: string;
  path: string;
  reason: string;
}

export interface Diagnostics {
  version: string;
  dataDir: string;
  language: string;
  lastSyncAt: number | null;
  sources: SourceRow[];
  watched: string[];
  unresolved: Unresolved[];
  files: FileReport[];
  logTail: string[];
}

export interface FfmpegStatus {
  available: boolean;
  path: string | null;
  downloading: boolean;
  error: string | null;
  downloaded: number;
  total: number | null;
}

export interface UpdateStatus {
  current: string;
  available: string | null;
  notes: string | null;
  busy: boolean;
  error: string | null;
  checkedAt: number | null;
}
