import { invoke } from "@tauri-apps/api/core";
import type {
  Diagnostics,
  FfmpegStatus,
  GameDetail,
  GameRow,
  Locale,
  LocaleInfo,
  PresetInfo,
  PreviewKind,
  Settings,
  SoundEvent,
  SoundPack,
  UpdateStatus,
  UserFolder,
} from "../types/domain";

const COMMANDS = {
  listGames: "list_games",
  gameDetail: "game_detail",
  getSettings: "get_settings",
  saveSettings: "save_settings",
  listPresets: "list_presets",
  previewPreset: "preview_preset",
  listSounds: "list_sounds",
  previewSound: "preview_sound",
  openUserFolder: "open_user_folder",
  testNotification: "test_notification",
  rescan: "rescan",
  diagnostics: "diagnostics",
  exportDiagnostics: "export_diagnostics",
  openPath: "open_path",
  overlayGame: "overlay_game",
  hotkeyError: "hotkey_error",
  toggleOverlay: "toggle_overlay",
  ffmpegStatus: "ffmpeg_status",
  downloadFfmpeg: "download_ffmpeg",
  cancelFfmpeg: "cancel_ffmpeg",
  updateStatus: "update_status",
  checkUpdate: "check_update",
  installUpdate: "install_update",
  uiLocale: "ui_locale",
  supportedLocales: "supported_locales",
} as const;

export const api = {
  listGames: () => invoke<GameRow[]>(COMMANDS.listGames),
  gameDetail: (appId: number) => invoke<GameDetail>(COMMANDS.gameDetail, { appId }),
  getSettings: () => invoke<Settings>(COMMANDS.getSettings),
  saveSettings: (settings: Settings) => invoke<Settings>(COMMANDS.saveSettings, { settings }),
  listPresets: () => invoke<PresetInfo[]>(COMMANDS.listPresets),
  previewPreset: (id: string, kind: PreviewKind) =>
    invoke<void>(COMMANDS.previewPreset, { id, kind }),
  listSounds: () => invoke<SoundPack[]>(COMMANDS.listSounds),
  previewSound: (id: string, event: SoundEvent) =>
    invoke<number | null>(COMMANDS.previewSound, { id, event }),
  openUserFolder: (kind: UserFolder) => invoke<void>(COMMANDS.openUserFolder, { kind }),
  testNotification: () => invoke<void>(COMMANDS.testNotification),
  rescan: () => invoke<boolean>(COMMANDS.rescan),
  diagnostics: () => invoke<Diagnostics>(COMMANDS.diagnostics),
  exportDiagnostics: () => invoke<string>(COMMANDS.exportDiagnostics),
  openPath: (path: string) => invoke<void>(COMMANDS.openPath, { path }),
  overlayGame: () => invoke<GameDetail | null>(COMMANDS.overlayGame),
  hotkeyError: () => invoke<string | null>(COMMANDS.hotkeyError),
  toggleOverlay: () => invoke<void>(COMMANDS.toggleOverlay),
  ffmpegStatus: () => invoke<FfmpegStatus>(COMMANDS.ffmpegStatus),
  downloadFfmpeg: () => invoke<void>(COMMANDS.downloadFfmpeg),
  cancelFfmpeg: () => invoke<void>(COMMANDS.cancelFfmpeg),
  updateStatus: () => invoke<UpdateStatus>(COMMANDS.updateStatus),
  checkUpdate: () => invoke<void>(COMMANDS.checkUpdate),
  installUpdate: () => invoke<void>(COMMANDS.installUpdate),
  uiLocale: () => invoke<Locale>(COMMANDS.uiLocale),
  supportedLocales: () => invoke<LocaleInfo[]>(COMMANDS.supportedLocales),
};

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
