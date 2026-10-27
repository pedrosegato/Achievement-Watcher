use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use aw_core::cloud::device::{LocalDevice, default_device_name, this_machine_id};
use aw_core::discovery::detect::DetectConfig;
use aw_core::discovery::folders::KnownFolders;
use aw_core::discovery::steam::steam_install_path;
use aw_core::engine::{CoreSettings, Engine, NewUnlock, ProgressUpdate, SyncOutcome};
use aw_core::gamesdb::{self, GamesDb};
use aw_core::lang::steam_language;
use aw_core::metadata::SteamMeta;
use aw_core::platform::{ExternalId, is_synthetic};
use aw_core::store::Store;
use aw_core::watcher::{self, WatchEvent, WatchOptions, WatcherHandle, unix_now};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use tracing_appender::non_blocking::WorkerGuard;

use crate::cloud::{self, CloudHandle, CloudJob};
use crate::diag;
use crate::games::{self, RunningGames};
use crate::i18n::{self, keys, text, text_with};
use crate::launch::PlayFlow;
use crate::overlay;
use crate::popup::{self, DEFAULT_ICON, PopupKind, PopupRequest, classify};
use crate::popup_queue::PopupQueue;
use crate::settings::Settings;
use crate::sounds::{self, SoundEvent};
use crate::updater::{self, UpdaterState};
use crate::video::{self, VideoState};

pub const MAIN_LABEL: &str = "main";
const TRAY_ID: &str = "main";
pub const GAMES_CHANGED: &str = "games-changed";
pub const PLAY_CHANGED: &str = "play-changed";
pub const SETTINGS_CHANGED: &str = "settings-changed";
pub const SCAN_PROGRESS: &str = "scan-progress";
const QUICK_META_TIMEOUT: Duration = Duration::from_secs(3);
const IDENTIFY_RETRY: Duration = Duration::from_secs(10 * 60);

pub struct AppPaths {
    pub data_dir: PathBuf,
    pub settings_file: PathBuf,
    pub db_file: PathBuf,
    pub cache_dir: PathBuf,
    pub user_presets: PathBuf,
    pub user_sounds: PathBuf,
    pub logs_dir: PathBuf,
    pub builtin_presets: PathBuf,
    pub builtin_sounds: PathBuf,
    pub device_file: PathBuf,
}

const MAX_PLATINUM_ICONS: usize = 64;

pub struct AppState {
    pub paths: AppPaths,
    pub folders: KnownFolders,
    pub settings: Mutex<Settings>,
    pub store: Mutex<Store>,
    pub watcher: Mutex<Option<WatcherHandle>>,
    pub popups: Arc<PopupQueue>,
    pub meta_jobs: Mutex<Sender<MetaJob>>,
    pub log_guard: Mutex<Option<WorkerGuard>>,
    pub running: Mutex<RunningGames>,
    pub game_pids: Mutex<HashMap<u32, u32>>,
    pub hotkey_error: Mutex<Option<String>>,
    pub video: VideoState,
    pub updater: UpdaterState,
    pub device: Mutex<LocalDevice>,
    pub cloud: CloudHandle,
    pub launching: Mutex<HashSet<u32>>,
    pub exiting: Mutex<HashSet<u32>>,
    pub plays: Mutex<HashMap<u32, PlayFlow>>,
}

pub enum MetaJob {
    Ensure(u32),
    Art(u32),
    Identify(ExternalId),
    Reset,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AppState {
    pub fn hotkey_error(&self) -> MutexGuard<'_, Option<String>> {
        lock(&self.hotkey_error)
    }

    pub fn current_game(&self) -> Option<u32> {
        if let Some((app_id, _)) = self.running().iter().max_by_key(|(_, game)| game.since) {
            return Some(*app_id);
        }
        self.store()
            .games_overview()
            .ok()?
            .into_iter()
            .max_by_key(|g| g.last_unlock_at.max(g.last_played))
            .filter(|g| g.last_unlock_at.is_some() || g.last_played.is_some())
            .map(|g| g.app_id)
    }

    pub fn running(&self) -> MutexGuard<'_, RunningGames> {
        lock(&self.running)
    }

    pub fn launching(&self) -> MutexGuard<'_, HashSet<u32>> {
        lock(&self.launching)
    }

    pub fn exiting(&self) -> MutexGuard<'_, HashSet<u32>> {
        lock(&self.exiting)
    }

    pub fn plays(&self) -> MutexGuard<'_, HashMap<u32, PlayFlow>> {
        lock(&self.plays)
    }

    pub fn set_game_pids(&self, pids: &HashMap<u32, u32>) {
        lock(&self.game_pids).clone_from(pids);
    }

    pub fn pids_of(&self, app_id: u32) -> HashSet<u32> {
        lock(&self.game_pids)
            .iter()
            .filter(|(_, id)| **id == app_id)
            .map(|(pid, _)| *pid)
            .collect()
    }

    pub fn flush_logs(&self) {
        drop(lock(&self.log_guard).take());
    }

    pub fn settings_snapshot(&self) -> Settings {
        lock(&self.settings).clone()
    }

    pub fn store(&self) -> MutexGuard<'_, Store> {
        lock(&self.store)
    }

    pub fn send_popup(&self, request: PopupRequest) {
        self.popups.push(request);
    }

    pub fn send_preview(&self, request: PopupRequest) {
        self.popups.push_preview(request);
    }

    pub fn ui_locale(&self) -> &'static str {
        i18n::resolve(&lock(&self.settings).ui_language)
    }

    pub fn sound_source(&self, pack_id: &str, event: SoundEvent) -> Option<PathBuf> {
        let builtin = &self.paths.builtin_sounds;
        let user = &self.paths.user_sounds;
        sounds::find(builtin, user, pack_id)
            .or_else(|| sounds::find(builtin, user, sounds::DEFAULT_PACK))
            .and_then(|pack| pack.source(event))
    }

    pub fn send_meta(&self, job: MetaJob) {
        let _ = lock(&self.meta_jobs).send(job);
    }

    pub fn with_watcher<T>(&self, f: impl FnOnce(&WatcherHandle) -> T) -> Option<T> {
        lock(&self.watcher).as_ref().map(f)
    }

    pub fn language(&self) -> String {
        lock(&self.settings)
            .language
            .clone()
            .filter(|l| !l.trim().is_empty())
            .unwrap_or_else(|| steam_language().to_owned())
    }

    pub fn apply_settings(&self, app: &AppHandle, new: Settings) -> Result<(), String> {
        let old = self.settings_snapshot();
        let new = self.apply_inner(app, new)?;
        self.cloud.send(CloudJob::SettingsChanged {
            old: Box::new(old),
            new: Box::new(new),
        });
        Ok(())
    }

    pub fn apply_cloud_settings(&self, app: &AppHandle, new: Settings) -> Result<(), String> {
        self.apply_inner(app, new).map(|_| ())
    }

    fn apply_inner(&self, app: &AppHandle, new: Settings) -> Result<Settings, String> {
        let new = new.sanitized();
        let old = self.settings_snapshot();
        new.save(&self.paths.settings_file)
            .map_err(|e| e.to_string())?;
        *lock(&self.settings) = new.clone();
        if old.autostart != new.autostart {
            apply_autostart(app, new.autostart);
        }
        if old.ui_language != new.ui_language {
            refresh_tray(app);
        }
        if old.video && new.video && video::capture_changed(&old, &new) {
            video::stop(app, None);
        }
        if new.video && (!old.video || video::capture_changed(&old, &new)) {
            let running: Vec<u32> = self.running().keys().copied().collect();
            for app_id in running {
                video::start(app, app_id);
            }
        } else if old.video && !new.video {
            video::stop(app, None);
        }
        if old.overlay_hotkey != new.overlay_hotkey {
            overlay::apply_hotkey(app, &new.overlay_hotkey);
        }
        if old.language != new.language {
            self.send_meta(MetaJob::Reset);
            queue_all_metadata(self);
        }
        if core_settings(&old) != core_settings(&new) {
            self.with_watcher(|w| w.update_settings(core_settings(&new)));
        }
        let _ = app.emit(SETTINGS_CHANGED, &new);
        Ok(new)
    }
}

pub fn core_settings(settings: &Settings) -> CoreSettings {
    CoreSettings {
        silent_after_minutes: settings.silent_after_minutes,
        detect: DetectConfig {
            steam: (settings.read_steam_libraries || settings.steam_achievements)
                .then(steam_install_path)
                .flatten(),
            steam_libraries: settings.read_steam_libraries,
            steam_stats: settings.steam_achievements,
            games_roots: settings.extra_game_roots.clone(),
            game_dirs: settings.extra_game_dirs.clone(),
        },
    }
}

fn apply_autostart(app: &AppHandle, enabled: bool) {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    if let Err(error) = result {
        tracing::warn!("autostart change failed: {error}");
    }
}

fn builtin_dir(app: &App, name: &str) -> PathBuf {
    let bundled = app
        .path()
        .resource_dir()
        .map(|dir| dir.join(name))
        .ok()
        .filter(|dir| dir.is_dir());
    bundled.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(name)
    })
}

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let paths = AppPaths {
        settings_file: data_dir.join("settings.json"),
        db_file: data_dir.join("data.sqlite"),
        cache_dir: data_dir.join("cache"),
        user_presets: data_dir.join("presets"),
        user_sounds: data_dir.join("sounds"),
        logs_dir: data_dir.join("logs"),
        builtin_presets: builtin_dir(app, "presets"),
        builtin_sounds: builtin_dir(app, "sounds"),
        device_file: data_dir.join("device.json"),
        data_dir,
    };
    for dir in [
        &paths.data_dir,
        &paths.cache_dir,
        &paths.user_presets,
        &paths.user_sounds,
        &paths.logs_dir,
    ] {
        std::fs::create_dir_all(dir)?;
    }
    let log_guard = diag::init_logging(&paths.logs_dir);
    tracing::info!("starting Achievement Watcher {}", env!("CARGO_PKG_VERSION"));

    let (settings, first_run) = Settings::load(&paths.settings_file);
    if first_run {
        if let Err(error) = settings.save(&paths.settings_file) {
            tracing::warn!("cannot save default settings: {error}");
        }
    }
    let device = LocalDevice::load(
        &paths.device_file,
        &default_device_name(),
        this_machine_id(),
    )?;
    let read_store = Store::open(&paths.db_file)?;
    let engine_store = Store::open(&paths.db_file)?;
    let folders = KnownFolders::from_system().unwrap_or_else(|error| {
        tracing::error!("no known folder is available: {error}");
        KnownFolders::default()
    });

    let popup_queue = Arc::new(PopupQueue::new());
    let (cloud_tx, cloud_rx) = channel::<CloudJob>();
    let (meta_tx, meta_rx) = channel::<MetaJob>();
    let preset = settings.preset.clone();
    let core = core_settings(&settings);
    app.manage(AppState {
        paths,
        folders: folders.clone(),
        settings: Mutex::new(settings.clone()),
        store: Mutex::new(read_store),
        watcher: Mutex::new(None),
        popups: Arc::clone(&popup_queue),
        meta_jobs: Mutex::new(meta_tx),
        log_guard: Mutex::new(log_guard),
        running: Mutex::new(RunningGames::new()),
        game_pids: Mutex::new(HashMap::new()),
        hotkey_error: Mutex::new(None),
        video: VideoState::default(),
        updater: UpdaterState::default(),
        device: Mutex::new(device),
        cloud: CloudHandle::new(cloud_tx.clone()),
        launching: Mutex::new(HashSet::new()),
        exiting: Mutex::new(HashSet::new()),
        plays: Mutex::new(HashMap::new()),
    });

    let handle = app.handle().clone();
    apply_autostart(&handle, settings.autostart);
    if let Err(error) = popup::create_window(&handle, &preset) {
        tracing::error!("cannot create popup window: {error}");
    }
    spawn_named("aw-popups", {
        let handle = handle.clone();
        move || popup::run_worker(handle, popup_queue)
    });
    spawn_named("aw-metadata", {
        let handle = handle.clone();
        move || run_metadata(handle, meta_rx)
    });

    let (events_tx, events_rx) = channel::<WatchEvent>();
    let engine = Engine::new(engine_store, folders, core);
    let watcher = watcher::start(engine, events_tx, WatchOptions::default());
    *lock(&app.state::<AppState>().watcher) = Some(watcher);
    spawn_named("aw-core-events", {
        let handle = handle.clone();
        move || run_core_events(handle, events_rx)
    });

    video::prepare(&handle);
    spawn_named("aw-cloud", {
        let handle = handle.clone();
        move || cloud::run_cloud_worker(handle, cloud_rx, cloud_tx)
    });
    app.state::<AppState>().cloud.send(CloudJob::Startup);
    spawn_named("aw-processes", {
        let handle = handle.clone();
        move || games::run_process_monitor(handle)
    });

    overlay::apply_hotkey(&handle, &settings.overlay_hotkey);
    updater::spawn(handle.clone());
    build_tray(app)?;
    let autostarted = std::env::args().any(|a| a == "--autostart");
    if !autostarted {
        show_main(&handle);
    }
    Ok(())
}

fn spawn_named(name: &str, f: impl FnOnce() + Send + 'static) {
    if let Err(error) = std::thread::Builder::new().name(name.into()).spawn(f) {
        tracing::error!("cannot start thread {name}: {error}");
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn tray_menu<M: Manager<tauri::Wry>>(manager: &M, locale: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let open = MenuItem::with_id(
        manager,
        "open",
        text(locale, keys::TRAY_OPEN),
        true,
        None::<&str>,
    )?;
    let test = MenuItem::with_id(
        manager,
        "test",
        text(locale, keys::TRAY_TEST),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        manager,
        "quit",
        text(locale, keys::TRAY_QUIT),
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(manager)?;
    Menu::with_items(manager, &[&open, &test, &separator, &quit])
}

pub fn refresh_tray(app: &AppHandle) {
    let locale = app.state::<AppState>().ui_locale();
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), tray_menu(app, locale)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn build_tray(app: &App) -> tauri::Result<()> {
    let menu = tray_menu(app, app.state::<AppState>().ui_locale())?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Achievement Watcher")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "test" => send_test_popup(app, None, PopupKind::Unlock, false),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn test_request(
    locale: &str,
    kind: PopupKind,
    preset: Option<String>,
    preview: bool,
) -> PopupRequest {
    let percent = match kind {
        PopupKind::Unlock | PopupKind::Progress => Some(48.0),
        PopupKind::Rare => Some(4.2),
        PopupKind::Platinum => Some(12.5),
    };
    PopupRequest {
        game: "Achievement Watcher".into(),
        display_name: text(locale, keys::TEST_TITLE),
        description: text(locale, keys::TEST_DESCRIPTION),
        icon_path: DEFAULT_ICON.into(),
        screenshot: false,
        preset_override: preset,
        kind,
        percent,
        preview,
        app_id: 0,
        api_name: "AW_TEST".into(),
        progress: (kind == PopupKind::Progress).then_some((75, 100)),
        total_achievements: 49,
        achievement_icons: Vec::new(),
    }
}

pub fn send_test_popup(app: &AppHandle, preset: Option<String>, kind: PopupKind, preview: bool) {
    let state = app.state::<AppState>();
    let request = test_request(state.ui_locale(), kind, preset, preview);
    if preview {
        state.send_preview(request);
    } else {
        state.send_popup(request);
    }
}

fn run_core_events(app: AppHandle, events: Receiver<WatchEvent>) {
    for event in events {
        match event {
            WatchEvent::Synced(outcome) => handle_synced(&app, &outcome),
            WatchEvent::ScanProgress(percent) => {
                let _ = app.emit(SCAN_PROGRESS, percent);
            }
            WatchEvent::Error(error) => tracing::error!("{error}"),
        }
    }
}

fn handle_synced(app: &AppHandle, outcome: &SyncOutcome) {
    let state = app.state::<AppState>();
    if outcome.first_boot && !outcome.new_unlocks.is_empty() {
        let games: BTreeSet<u32> = outcome.new_unlocks.iter().map(|u| u.app_id).collect();
        tracing::info!(
            "first boot: imported {} achievements in {} games",
            outcome.new_unlocks.len(),
            games.len()
        );
        let locale = state.ui_locale();
        state.send_popup(PopupRequest {
            game: "Achievement Watcher".into(),
            display_name: text_with(
                locale,
                keys::FIRST_BOOT_TITLE,
                &[
                    ("achievements", outcome.new_unlocks.len().to_string()),
                    ("games", games.len().to_string()),
                ],
            ),
            description: text(locale, keys::FIRST_BOOT_DESCRIPTION),
            icon_path: DEFAULT_ICON.into(),
            screenshot: false,
            preset_override: None,
            kind: PopupKind::Unlock,
            percent: None,
            preview: false,
            app_id: 0,
            api_name: "AW_FIRST_BOOT".into(),
            progress: None,
            total_achievements: 0,
            achievement_icons: Vec::new(),
        });
    }
    let now = aw_core::watcher::unix_now();
    for unlock in outcome.new_unlocks.iter().filter(|u| u.notify) {
        if let Some(seconds) = crate::games::playtime_now(&state, unlock.app_id, now)
            && let Err(error) =
                state
                    .store()
                    .set_unlock_playtime(unlock.app_id, &unlock.api_name, seconds)
        {
            tracing::warn!("cannot save unlock playtime: {error}");
        }
    }
    let steam_popups = state.settings_snapshot().steam_popups;
    let notified: Vec<&NewUnlock> = outcome
        .new_unlocks
        .iter()
        .filter(|u| u.notify && (u.source_id != "steam" || steam_popups))
        .collect();
    for (index, unlock) in notified.iter().enumerate() {
        tracing::info!("new unlock {} {}", unlock.app_id, unlock.api_name);
        let last_for_game = !notified[index + 1..]
            .iter()
            .any(|later| later.app_id == unlock.app_id);
        let request = build_request(app, unlock, last_for_game);
        video::clip(
            app,
            unlock.app_id,
            request.game.clone(),
            request.display_name.clone(),
        );
        state.send_popup(request);
    }
    if state.settings_snapshot().progress_popups {
        for update in &outcome.progress {
            state.send_popup(build_progress_request(app, update));
        }
    }
    if outcome.first_boot || !outcome.new_unlocks.is_empty() {
        let _ = app.emit(GAMES_CHANGED, ());
    }
    for key in &outcome.pending {
        state.send_meta(MetaJob::Identify(key.clone()));
    }
    queue_all_metadata(&state);
}

fn queue_all_metadata(state: &AppState) {
    let ids: Vec<u32> = state
        .store()
        .games_overview()
        .map(|games| games.into_iter().map(|g| g.app_id).collect())
        .unwrap_or_default();
    for id in &ids {
        state.send_meta(MetaJob::Ensure(*id));
    }
    for id in ids {
        state.send_meta(MetaJob::Art(id));
    }
}

fn fetch_game_art(
    meta: &SteamMeta,
    store: &Store,
    steam: Option<&Path>,
    cache_dir: &Path,
    app_id: u32,
) -> bool {
    if !is_synthetic(app_id) {
        return crate::art::fetch_art(meta, steam, cache_dir, app_id);
    }
    store
        .external_game(app_id)
        .ok()
        .flatten()
        .is_some_and(|game| crate::art::fetch_external_art(meta, cache_dir, app_id, &game.images))
}

fn build_request(app: &AppHandle, unlock: &NewUnlock, last_for_game: bool) -> PopupRequest {
    let state = app.state::<AppState>();
    let lang = state.language();
    let missing = state
        .store()
        .schema_info(unlock.app_id)
        .ok()
        .flatten()
        .is_none();
    if missing {
        let meta =
            SteamMeta::new(state.paths.cache_dir.clone(), &lang).with_timeout(QUICK_META_TIMEOUT);
        if let Ok(store) = Store::open(&state.paths.db_file)
            && let Err(error) = meta.ensure(&store, unlock.app_id, unix_now())
        {
            tracing::warn!("metadata for {} not available: {error}", unlock.app_id);
        }
    }
    let store = state.store();
    let row = store
        .schema_row(unlock.app_id, &unlock.api_name)
        .ok()
        .flatten();
    let game = store
        .game_name(unlock.app_id)
        .ok()
        .flatten()
        .unwrap_or_else(|| format!("App {}", unlock.app_id));
    let quick =
        SteamMeta::new(state.paths.cache_dir.clone(), &lang).with_timeout(QUICK_META_TIMEOUT);
    let icon_path = row
        .as_ref()
        .and_then(|r| r.icon.as_deref())
        .filter(|file| quick.ensure_icon(unlock.app_id, file))
        .map_or_else(
            || DEFAULT_ICON.to_owned(),
            |file| format!("aw-icon/{}/{file}", unlock.app_id),
        );
    let overview = store
        .games_overview()
        .ok()
        .and_then(|games| games.into_iter().find(|g| g.app_id == unlock.app_id));
    let total = overview.as_ref().map_or(0, |g| g.total);
    let completes_game =
        last_for_game && overview.is_some_and(|g| g.total > 0 && g.unlocked >= g.total);
    let percent = row.as_ref().and_then(|r| r.percent);
    let threshold = state.settings_snapshot().rare_threshold;
    PopupRequest {
        game,
        display_name: row
            .as_ref()
            .map_or_else(|| unlock.api_name.clone(), |r| r.display_name.clone()),
        description: row.map(|r| r.description).unwrap_or_default(),
        icon_path,
        screenshot: true,
        preset_override: None,
        kind: classify(percent, threshold, completes_game),
        percent,
        preview: false,
        app_id: unlock.app_id,
        api_name: unlock.api_name.clone(),
        progress: None,
        total_achievements: total,
        achievement_icons: if completes_game {
            game_icons(&state, &store, unlock.app_id)
        } else {
            Vec::new()
        },
    }
}

fn game_icons(state: &AppState, store: &Store, app_id: u32) -> Vec<String> {
    store
        .game_achievements(app_id)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| row.icon)
        .filter(|file| {
            state
                .paths
                .cache_dir
                .join("icons")
                .join(app_id.to_string())
                .join(file)
                .is_file()
        })
        .take(MAX_PLATINUM_ICONS)
        .map(|file| format!("aw-icon/{app_id}/{file}"))
        .collect()
}

fn build_progress_request(app: &AppHandle, update: &ProgressUpdate) -> PopupRequest {
    let state = app.state::<AppState>();
    let store = state.store();
    let row = store
        .schema_row(update.app_id, &update.api_name)
        .ok()
        .flatten();
    let game = store
        .game_name(update.app_id)
        .ok()
        .flatten()
        .unwrap_or_else(|| format!("App {}", update.app_id));
    let icon_path = row
        .as_ref()
        .and_then(|r| r.icon_gray.as_deref().or(r.icon.as_deref()))
        .filter(|file| {
            state
                .paths
                .cache_dir
                .join("icons")
                .join(update.app_id.to_string())
                .join(file)
                .is_file()
        })
        .map_or_else(
            || DEFAULT_ICON.to_owned(),
            |file| format!("aw-icon/{}/{file}", update.app_id),
        );
    PopupRequest {
        game,
        display_name: row
            .as_ref()
            .map_or_else(|| update.api_name.clone(), |r| r.display_name.clone()),
        description: format!("{} / {}", update.current, update.max),
        icon_path,
        screenshot: false,
        preset_override: None,
        kind: PopupKind::Progress,
        percent: row.and_then(|r| r.percent),
        preview: false,
        app_id: update.app_id,
        api_name: update.api_name.clone(),
        progress: Some((update.current, update.max)),
        total_achievements: 0,
        achievement_icons: Vec::new(),
    }
}

fn run_metadata(app: AppHandle, jobs: Receiver<MetaJob>) {
    let state = app.state::<AppState>();
    let store = match Store::open(&state.paths.db_file) {
        Ok(store) => store,
        Err(error) => {
            tracing::error!("metadata store unavailable: {error}");
            return;
        }
    };
    let steam = steam_install_path();
    let mut done: HashSet<u32> = HashSet::new();
    let mut art_seen: HashSet<u32> = HashSet::new();
    let mut icons: VecDeque<u32> = VecDeque::new();
    let mut arts: VecDeque<u32> = VecDeque::new();
    let games_db = GamesDb::new();
    let mut identify_failed: HashMap<ExternalId, Instant> = HashMap::new();
    loop {
        let job = if icons.is_empty() && arts.is_empty() {
            match jobs.recv() {
                Ok(job) => Some(job),
                Err(_) => return,
            }
        } else {
            match jobs.try_recv() {
                Ok(job) => Some(job),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
            }
        };
        let meta = SteamMeta::new(state.paths.cache_dir.clone(), &state.language());
        match job {
            Some(MetaJob::Reset) => done.clear(),
            Some(MetaJob::Ensure(app_id)) => {
                if !done.insert(app_id) {
                    continue;
                }
                match meta.ensure(&store, app_id, unix_now()) {
                    Ok(aw_core::metadata::Ensured::Fetched) => {
                        icons.push_back(app_id);
                        state.with_watcher(WatcherHandle::request_sync);
                        let _ = app.emit(GAMES_CHANGED, ());
                    }
                    Ok(_) => icons.push_back(app_id),
                    Err(error) => {
                        tracing::warn!("metadata for {app_id}: {error}");
                        done.remove(&app_id);
                    }
                }
            }
            Some(MetaJob::Identify(key)) => {
                let resolved = store.external_app_id(&key).ok().flatten().is_some();
                let waiting = identify_failed
                    .get(&key)
                    .is_some_and(|at| at.elapsed() < IDENTIFY_RETRY);
                if resolved || waiting {
                    continue;
                }
                match gamesdb::resolve(&store, &games_db, &key, unix_now()) {
                    Ok(app_id) => {
                        identify_failed.remove(&key);
                        tracing::info!("{} {} is app {app_id}", key.platform.as_str(), key.id);
                        state.with_watcher(WatcherHandle::request_sync);
                        state.send_meta(MetaJob::Ensure(app_id));
                        state.send_meta(MetaJob::Art(app_id));
                        let _ = app.emit(GAMES_CHANGED, ());
                    }
                    Err(error) => {
                        tracing::warn!(
                            "cannot identify {} {}: {error}",
                            key.platform.as_str(),
                            key.id
                        );
                        identify_failed.insert(key, Instant::now());
                    }
                }
            }
            Some(MetaJob::Art(app_id)) => {
                if art_seen.insert(app_id)
                    && crate::art::needs_fetch(steam.as_deref(), &state.paths.cache_dir, app_id)
                {
                    arts.push_back(app_id);
                }
            }
            None if !arts.is_empty() => {
                if let Some(app_id) = arts.pop_front()
                    && fetch_game_art(
                        &meta,
                        &store,
                        steam.as_deref(),
                        &state.paths.cache_dir,
                        app_id,
                    )
                {
                    let _ = app.emit(GAMES_CHANGED, ());
                }
            }
            None => {
                if let Some(app_id) = icons.pop_front()
                    && let Err(error) = meta.ensure_icons(&store, app_id)
                {
                    tracing::warn!("icons for {app_id}: {error}");
                }
            }
        }
    }
}
