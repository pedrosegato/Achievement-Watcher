import type {
  ACHIEVEMENT_FILTERS,
  POSITIONS,
  PREVIEW_KINDS,
  SOUND_EVENTS,
  THEMES,
  USER_FOLDERS,
  VIDEO_FORMATS,
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
export type VideoFormat = (typeof VIDEO_FORMATS)[number];
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
  videoFormat: VideoFormat;
  autoUpdate: boolean;
  ffmpegPath: string | null;
  videoDir: string | null;
  hideHiddenAchievements: boolean;
  uiLanguage: UiLanguage;
  theme: Theme;
  cloudFolder: string | null;
  gameSync: Record<string, GameSyncPref>;
}

export interface GameSyncPref {
  enabled: boolean | null;
  savePath: string | null;
}

export type DefaultSync = "on" | "steamCloud" | "unknown";
export type CloudFileState = "inCloud" | "inFolder";
export type HeadReason = "push" | "restore" | "branch" | "manual" | "undo";
export type CloudLocated = "found" | "notFound" | "registry" | "unsupported" | "waitingManifest";

export interface CloudGameSize {
  appId: number | null;
  key: string;
  name: string;
  bytes: number;
}

export interface CloudOverview {
  folder: string | null;
  folderMissing: boolean;
  deviceName: string;
  suggestions: string[];
  totalBytes: number;
  games: CloudGameSize[];
  manifestReady: boolean;
  lastError: string | null;
}

export interface LastAction {
  reason: HeadReason;
  at: number;
  deviceName: string;
  canUndo: boolean;
}

export interface SnapshotView {
  id: string;
  parent: string | null;
  createdAt: number;
  deviceName: string;
  playtimeSeconds: number | null;
  unlocked: number | null;
  isPrincipal: boolean;
  foundOnDevice: boolean;
}

export interface CloudGameView {
  key: string | null;
  enabled: boolean;
  default: DefaultSync;
  located: CloudLocated;
  paths: string[];
  oversize: boolean;
  state: CloudFileState | null;
  lastSnapshotAt: number | null;
  openElsewhere: string | null;
  lastAction: LastAction | null;
  snapshots: SnapshotView[];
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

export interface ExternalId {
  platform: string;
  id: string;
}

export interface ExternalGame {
  platform: string;
  external_id: string;
  app_id: number;
  title: string | null;
  resolved_at: number;
}

export interface Diagnostics {
  version: string;
  dataDir: string;
  language: string;
  lastSyncAt: number | null;
  sources: SourceRow[];
  watched: string[];
  unresolved: Unresolved[];
  pending: ExternalId[];
  external: ExternalGame[];
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

export type PlayState =
  | { type: "idle"; needsExe: boolean }
  | { type: "syncing"; stuck: boolean }
  | { type: "launching" }
  | { type: "running" }
  | { type: "exiting" }
  | { type: "failed"; reason: string }
  | { type: "openElsewhere"; device: string; since: number };

export type PlayResponse =
  | { type: "started" }
  | { type: "openElsewhere"; device: string; since: number }
  | { type: "needsExe" }
  | { type: "alreadyRunning" };

export interface LaunchChoice {
  exe: string | null;
  chosen: boolean;
  steam: boolean;
}
