use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant, SystemTime};

use aw_core::cloud::capture::HashCache;
use aw_core::cloud::device::{DeviceRecord, LocalDevice};
use aw_core::cloud::heads::{principal, read_heads};
use aw_core::cloud::identity::{push_identities, restore_identities};
use aw_core::cloud::key::{CloudKey, GameKind, cloud_key_for, game_kind};
use aw_core::cloud::keyed::{self, KeyedDoc, stamp_changes};
use aw_core::cloud::layout::{CloudLayout, read_json, write_json_atomic};
use aw_core::cloud::locate::{LocateInput, Located, SaveRoot, locate};
use aw_core::cloud::lock::{LOCK_RENEW_SECS, LockRecord, own_lock, remove_own_lock, write_lock};
use aw_core::cloud::manifest::{ManifestIndex, load_index, refresh};
use aw_core::cloud::portable::{Placeholders, PortablePath};
use aw_core::cloud::snapshot::{Contents, list_snapshots};
use aw_core::cloud::steamcloud::{
    DefaultSync, default_sync, fetch_has_cloud, has_autocloud_marker, has_steam_remotecache,
    sync_enabled,
};
use aw_core::cloud::sync::{CloudContext, GameTarget, SyncEvent, reconcile, set_principal, undo};
use aw_core::cloud::{CloudError, CloudResult};
use aw_core::discovery::Found;
use aw_core::discovery::running::KnownGame;
use aw_core::discovery::steam::{installed_apps, library_folders, steam_install_path};
use aw_core::store::Store;
use aw_core::watcher::unix_now;
use notify::{RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};

use crate::app::{AppState, GAMES_CHANGED, PLAY_CHANGED};
use crate::i18n::text;
use crate::settings::{GameSyncPref, Settings, synced_view};

pub const CLOUD_CHANGED: &str = "cloud-changed";
const SETTLE: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_secs(60);
const DEBOUNCE: Duration = Duration::from_secs(2);
const MANIFEST_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const STEAM_LOOKUP_SPACING: Duration = Duration::from_secs(2);
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(15);
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(120);
const README_KEY: &str = "cloud.readme";
const INDEX_FILE: &str = "ludusavi-index.json";

pub enum CloudJob {
    Startup,
    GameStarted(u32),
    GameExited(u32),
    ReconcileAll,
    SettingsChanged {
        old: Box<Settings>,
        new: Box<Settings>,
    },
    CloudFilesChanged,
    Undo(u32),
    SetPrincipal(u32, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestState {
    Pending,
    Ready,
    Unavailable,
}

pub fn inputs_ready(last_sync_at: Option<i64>, manifest: ManifestState) -> bool {
    last_sync_at.is_some() && manifest != ManifestState::Pending
}

pub fn cloud_only_allowed(contents: &Contents) -> bool {
    contents.save && !contents.state && !contents.emulator_files
}

pub struct CloudHandle {
    pub jobs: Mutex<Sender<CloudJob>>,
    pub manifest: RwLock<Option<Arc<ManifestIndex>>>,
    pub manifest_state: Mutex<ManifestState>,
    pub last_error: Mutex<Option<String>>,
    pub game_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    pub last_change: Arc<Mutex<Option<Instant>>>,
}

pub fn skip_for_sync(app_id: u32, running: &BTreeSet<u32>, launching: &HashSet<u32>) -> bool {
    running.contains(&app_id) || launching.contains(&app_id)
}

impl CloudHandle {
    pub fn new(jobs: Sender<CloudJob>) -> CloudHandle {
        CloudHandle {
            jobs: Mutex::new(jobs),
            manifest: RwLock::new(None),
            manifest_state: Mutex::new(ManifestState::Pending),
            last_error: Mutex::new(None),
            game_locks: Mutex::new(HashMap::new()),
            last_change: Arc::new(Mutex::new(None)),
        }
    }

    pub fn game_lock(&self, folder_key: &str) -> Arc<Mutex<()>> {
        Arc::clone(
            self.game_locks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(folder_key.to_owned())
                .or_default(),
        )
    }

    pub fn quiet_for(&self) -> Duration {
        self.last_change
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .map_or(Duration::MAX, |at| at.elapsed())
    }

    pub fn send(&self, job: CloudJob) {
        let _ = self
            .jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(job);
    }

    pub fn manifest(&self) -> Option<Arc<ManifestIndex>> {
        self.manifest
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn manifest_state(&self) -> ManifestState {
        *self
            .manifest_state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn set_manifest(&self, index: Option<ManifestIndex>, state: ManifestState) {
        if let Some(index) = index {
            *self
                .manifest
                .write()
                .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(index));
        }
        *self
            .manifest_state
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = state;
    }

    fn set_error(&self, error: Option<String>) {
        *self
            .last_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = error;
    }
}

pub fn cloud_layout(settings: &Settings) -> Option<CloudLayout> {
    settings.cloud_folder.clone().map(CloudLayout::new)
}

pub fn manual_path(settings: &Settings, key: &CloudKey) -> Option<PortablePath> {
    settings
        .game_sync
        .get(&key.as_folder_key())
        .and_then(|pref| pref.save_path.as_deref())
        .and_then(PortablePath::new)
}

pub fn user_choice(settings: &Settings, key: &CloudKey) -> Option<bool> {
    settings
        .game_sync
        .get(&key.as_folder_key())
        .and_then(|pref| pref.enabled)
}

pub struct Inventory {
    found: Vec<Found>,
    known: Vec<KnownGame>,
    installs: HashMap<u32, PathBuf>,
    steam_root: Option<PathBuf>,
    manifest: Option<Arc<ManifestIndex>>,
}

pub fn inventory(state: &AppState) -> Inventory {
    let snapshot = state.with_watcher(|w| w.snapshot()).unwrap_or_default();
    let steam_root = steam_install_path();
    let installs = steam_root
        .as_deref()
        .map(|steam| {
            library_folders(steam)
                .iter()
                .flat_map(|library| installed_apps(library))
                .map(|app| (app.app_id, app.install_dir))
                .collect()
        })
        .unwrap_or_default();
    Inventory {
        found: snapshot.found,
        known: snapshot.known_games,
        installs,
        steam_root,
        manifest: state.cloud.manifest(),
    }
}

impl Inventory {
    pub fn kind_of(&self, state: &AppState, app_id: u32) -> GameKind {
        let sources = state.store().game_sources(app_id).unwrap_or_default();
        game_kind(app_id, &self.found, &sources)
    }

    pub fn game_dir(&self, app_id: u32) -> Option<PathBuf> {
        self.known
            .iter()
            .find(|game| game.app_id == app_id)
            .map(|game| game.dir.clone())
            .or_else(|| self.installs.get(&app_id).cloned())
    }
}

pub struct TargetInfo {
    pub target: GameTarget,
    pub located: Located,
    pub enabled: bool,
    pub default: DefaultSync,
}

pub fn game_target(state: &AppState, inv: &Inventory, app_id: u32) -> Result<TargetInfo, String> {
    let key = cloud_key_for(app_id, &inv.found).ok_or_else(|| "noKey".to_owned())?;
    let settings = state.settings_snapshot();
    let (sources, name, steam_cloud) = {
        let store = state.store();
        (
            store.game_sources(app_id).unwrap_or_default(),
            store.game_name(app_id).ok().flatten(),
            store.steam_cloud(app_id).ok().flatten(),
        )
    };
    let kind = game_kind(app_id, &inv.found, &sources);
    let manifest = inv.manifest.as_ref().and_then(|m| m.games.get(&app_id));
    let install_dir = inv
        .known
        .iter()
        .find(|game| game.app_id == app_id)
        .map(|game| game.dir.clone())
        .or_else(|| inv.installs.get(&app_id).cloned());
    let placeholders = Placeholders {
        folders: Some(state.folders.clone()),
        install_dir,
        steam_root: inv.steam_root.clone(),
    };
    let emulator_files: Vec<PathBuf> = inv
        .found
        .iter()
        .filter(|f| f.app_id == app_id && f.source_id != "steam")
        .map(|f| f.path.clone())
        .collect();
    let manual = manual_path(&settings, &key);
    let located = locate(&LocateInput {
        app_id,
        kind,
        manifest,
        emulator_files: &emulator_files,
        sources: &sources,
        manual: manual.as_ref(),
        placeholders: &placeholders,
    });
    let roots = match &located {
        Located::Roots(roots) => roots.clone(),
        _ => Vec::new(),
    };
    let cloud_evidence = kind == GameKind::SteamOriginal
        && (manifest.is_some_and(|m| m.steam_cloud)
            || has_steam_remotecache(app_id, &placeholders)
            || has_autocloud_marker(&roots, &placeholders));
    let default = default_sync(kind, steam_cloud, cloud_evidence);
    let enabled = sync_enabled(default, user_choice(&settings, &key));
    Ok(TargetInfo {
        target: GameTarget {
            key: key.clone(),
            name: name
                .or_else(|| manifest.map(|m| m.name.clone()))
                .unwrap_or_else(|| key.as_folder_key()),
            app_id,
            kind,
            roots,
            placeholders,
        },
        located,
        enabled,
        default,
    })
}

fn cloud_only_target(
    state: &AppState,
    inv: &Inventory,
    layout: &CloudLayout,
    app_id: u32,
) -> Option<GameTarget> {
    let settings = state.settings_snapshot();
    let key = CloudKey::Steam(app_id);
    if user_choice(&settings, &key) == Some(false) {
        return None;
    }
    let manifest = inv.manifest.as_ref()?.games.get(&app_id)?;
    let games = layout.game_dirs(&key);
    let metas: Vec<_> = list_snapshots(&games).into_iter().map(|(_, m)| m).collect();
    let current = principal(&read_heads(&games), &metas)?;
    let meta = metas.iter().find(|m| m.id == current.snapshot_id)?;
    if !cloud_only_allowed(&meta.contents) {
        return None;
    }
    let name = meta.game_name.clone();
    let placeholders = Placeholders {
        folders: Some(state.folders.clone()),
        install_dir: None,
        steam_root: inv.steam_root.clone(),
    };
    let manual = manual_path(&settings, &key);
    let roots: Vec<SaveRoot> = match locate(&LocateInput {
        app_id,
        kind: GameKind::SteamOriginal,
        manifest: Some(manifest),
        emulator_files: &[],
        sources: &[],
        manual: manual.as_ref(),
        placeholders: &placeholders,
    }) {
        Located::Roots(roots) => roots,
        _ => return None,
    };
    Some(GameTarget {
        key,
        name,
        app_id,
        kind: GameKind::SteamOriginal,
        roots,
        placeholders,
    })
}

struct Worker {
    app: AppHandle,
    jobs: Sender<CloudJob>,
    store: Option<Store>,
    cache: HashCache,
    watcher: Option<notify::RecommendedWatcher>,
    watched: Option<PathBuf>,
    started: Option<PathBuf>,
    last_renew: i64,
}

pub fn run_cloud_worker(app: AppHandle, jobs: Receiver<CloudJob>, sender: Sender<CloudJob>) {
    let store = Store::open(&app.state::<AppState>().paths.db_file)
        .map_err(|error| tracing::error!("cloud worker cannot open the database: {error}"))
        .ok();
    let mut worker = Worker {
        app,
        jobs: sender,
        store,
        cache: HashCache::default(),
        watcher: None,
        watched: None,
        started: None,
        last_renew: 0,
    };
    loop {
        match jobs.recv_timeout(TICK) {
            Ok(CloudJob::CloudFilesChanged) => {
                while let Ok(job) = jobs.recv_timeout(DEBOUNCE) {
                    if !matches!(job, CloudJob::CloudFilesChanged) {
                        worker.handle(job);
                    }
                }
                worker.handle(CloudJob::CloudFilesChanged);
            }
            Ok(job) => worker.handle(job),
            Err(RecvTimeoutError::Timeout) => worker.tick(),
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

impl Worker {
    fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }

    fn layout(&self) -> Option<CloudLayout> {
        cloud_layout(&self.state().settings_snapshot())
    }

    fn device(&self) -> LocalDevice {
        self.state()
            .device
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn handle(&mut self, job: CloudJob) {
        let exited = match job {
            CloudJob::GameExited(app_id) => Some(app_id),
            _ => None,
        };
        self.handle_job(job);
        if let Some(app_id) = exited {
            self.state().exiting().remove(&app_id);
            let _ = self.app.emit(PLAY_CHANGED, app_id);
        }
    }

    fn handle_job(&mut self, job: CloudJob) {
        if let CloudJob::SettingsChanged { old, new } = job {
            self.settings_changed(&old, &new);
            return;
        }
        let Some(layout) = self.layout() else {
            return;
        };
        if !layout.exists() {
            self.state().cloud.set_error(Some("missingRoot".into()));
            self.started = None;
            self.watched = None;
            self.watcher = None;
            self.emit(None);
            return;
        }
        self.state().cloud.set_error(None);
        if self.started.as_deref() != Some(layout.root())
            && !matches!(job, CloudJob::Startup)
            && let Err(error) = self.startup(&layout)
        {
            tracing::warn!("cloud startup: {error}");
        }
        let result = match job {
            CloudJob::Startup => self.startup(&layout),
            CloudJob::GameStarted(app_id) => self.game_started(&layout, app_id),
            CloudJob::GameExited(app_id) => self.game_exited(&layout, app_id),
            CloudJob::ReconcileAll => self.reconcile_all(&layout),
            CloudJob::CloudFilesChanged => {
                self.merge_settings(&layout);
                self.reconcile_all(&layout)
            }
            CloudJob::Undo(app_id) => self.with_target(&layout, app_id, undo),
            CloudJob::SetPrincipal(app_id, id) => {
                self.with_target(&layout, app_id, |ctx, target| {
                    set_principal(ctx, target, &id, aw_core::cloud::heads::HeadReason::Manual)
                })
            }
            CloudJob::SettingsChanged { .. } => Ok(()),
        };
        if let Err(error) = result {
            tracing::warn!("cloud sync: {error}");
            let code = match error {
                CloudError::MissingRoot => "missingRoot".to_owned(),
                CloudError::MultipleAccounts(_) => "multipleAccounts".to_owned(),
                other => other.to_string(),
            };
            self.state().cloud.set_error(Some(code));
        }
        self.emit(None);
    }

    fn emit(&self, app_id: Option<u32>) {
        let _ = self.app.emit(CLOUD_CHANGED, app_id);
    }

    fn ready(&self) -> bool {
        let state = self.state();
        let last_sync = state.with_watcher(|w| w.snapshot().last_sync_at).flatten();
        inputs_ready(last_sync, state.cloud.manifest_state())
    }

    fn startup(&mut self, layout: &CloudLayout) -> CloudResult<()> {
        self.started = Some(layout.root().to_path_buf());
        let device = self.device();
        let now = unix_now();
        let locale = self.state().ui_locale();
        layout.write_readme(&text(locale, README_KEY))?;
        write_json_atomic(
            &layout.device_file(&device.id),
            &DeviceRecord {
                name: device.name.clone(),
                last_seen: now,
            },
        )?;
        self.ensure_settings_doc(layout, &device, now)?;
        self.merge_settings(layout);
        if let Some(store) = &self.store {
            let folders = self.state().folders.clone();
            if !store.has_any_cloud_base()? {
                restore_identities(layout, &folders, store, &device.name, now)?;
            }
            push_identities(layout, &folders, store, &device.name, now)?;
        }
        self.watch(layout);
        self.spawn_manifest_refresh();
        self.spawn_steam_cloud_lookups();
        self.reconcile_all(layout)
    }

    fn watch(&mut self, layout: &CloudLayout) {
        if self.watched.as_deref() == Some(layout.root()) {
            return;
        }
        let sender = self.jobs.clone();
        let last_change = Arc::clone(&self.state().cloud.last_change);
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok() {
                *last_change.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
                let _ = sender.send(CloudJob::CloudFilesChanged);
            }
        });
        match watcher {
            Ok(mut watcher) => match watcher.watch(layout.root(), RecursiveMode::Recursive) {
                Ok(()) => {
                    self.watcher = Some(watcher);
                    self.watched = Some(layout.root().to_path_buf());
                }
                Err(error) => tracing::warn!("cannot watch the cloud folder: {error}"),
            },
            Err(error) => tracing::warn!("cannot watch the cloud folder: {error}"),
        }
    }

    fn spawn_manifest_refresh(&self) {
        let app = self.app.clone();
        let jobs = self.jobs.clone();
        let _ = std::thread::Builder::new()
            .name("aw-ludusavi".into())
            .spawn(move || {
                let state = app.state::<AppState>();
                let cache = state.paths.cache_dir.clone();
                if state.cloud.manifest_state() == ManifestState::Pending
                    && let Some(index) = load_index(&cache)
                {
                    state.cloud.set_manifest(Some(index), ManifestState::Ready);
                    let _ = jobs.send(CloudJob::ReconcileAll);
                }
                let fresh = std::fs::metadata(cache.join(INDEX_FILE))
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| SystemTime::now().duration_since(t).ok())
                    .is_some_and(|age| age < MANIFEST_MAX_AGE);
                if !fresh {
                    let agent = ureq::Agent::config_builder()
                        .timeout_global(Some(MANIFEST_TIMEOUT))
                        .build()
                        .into();
                    if let Err(error) = refresh(&agent, &cache) {
                        tracing::warn!("cannot refresh the save list: {error}");
                    }
                }
                match load_index(&cache) {
                    Some(index) => state.cloud.set_manifest(Some(index), ManifestState::Ready),
                    None if state.cloud.manifest_state() == ManifestState::Pending => {
                        state.cloud.set_manifest(None, ManifestState::Unavailable);
                    }
                    None => {}
                }
                let _ = jobs.send(CloudJob::ReconcileAll);
            });
    }

    fn spawn_steam_cloud_lookups(&self) {
        let app = self.app.clone();
        let jobs = self.jobs.clone();
        let _ = std::thread::Builder::new()
            .name("aw-steamcloud".into())
            .spawn(move || {
                let state = app.state::<AppState>();
                let Ok(store) = Store::open(&state.paths.db_file) else {
                    return;
                };
                let found = state
                    .with_watcher(|w| w.snapshot().found)
                    .unwrap_or_default();
                let games: BTreeSet<u32> = store
                    .games_overview()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|g| g.app_id)
                    .filter(|id| !aw_core::platform::is_synthetic(*id))
                    .filter(|id| {
                        let sources = store.game_sources(*id).unwrap_or_default();
                        game_kind(*id, &found, &sources) == GameKind::SteamOriginal
                    })
                    .filter(|id| store.steam_cloud(*id).ok().flatten().is_none())
                    .collect();
                if games.is_empty() {
                    return;
                }
                let agent: ureq::Agent = ureq::Agent::config_builder()
                    .timeout_global(Some(LOOKUP_TIMEOUT))
                    .build()
                    .into();
                for app_id in games {
                    if let Ok(Some(has_cloud)) = fetch_has_cloud(&agent, app_id) {
                        let _ = store.set_steam_cloud(app_id, has_cloud, unix_now());
                    }
                    std::thread::sleep(STEAM_LOOKUP_SPACING);
                }
                let _ = jobs.send(CloudJob::ReconcileAll);
            });
    }

    fn ensure_settings_doc(
        &self,
        layout: &CloudLayout,
        device: &LocalDevice,
        now: i64,
    ) -> CloudResult<()> {
        let own = layout.settings_file(&device.id);
        if own.is_file() {
            return Ok(());
        }
        let others = std::fs::read_dir(layout.settings_dir())
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| entry.path() != own);
        let mut doc = KeyedDoc {
            device_id: device.id.0.clone(),
            ..KeyedDoc::default()
        };
        if !others {
            let current = synced_view(&self.state().settings_snapshot());
            stamp_changes(&mut doc, &Default::default(), &current, now);
        }
        write_json_atomic(&own, &doc)?;
        Ok(())
    }

    fn merge_settings(&self, layout: &CloudLayout) {
        let docs: Vec<KeyedDoc> = std::fs::read_dir(layout.settings_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| read_json::<KeyedDoc>(&entry.path()))
            .collect();
        if docs.is_empty() {
            return;
        }
        let state = self.state();
        let current = state.settings_snapshot();
        let mut value = serde_json::to_value(&current).unwrap_or_default();
        keyed::apply(&mut value, &keyed::merge(&docs));
        let Ok(merged) = serde_json::from_value::<Settings>(value) else {
            return;
        };
        let merged = merged.sanitized();
        if merged != current
            && let Err(error) = state.apply_cloud_settings(&self.app, merged)
        {
            tracing::warn!("cannot apply cloud settings: {error}");
        }
    }

    fn settings_changed(&mut self, old: &Settings, new: &Settings) {
        let Some(layout) = cloud_layout(new) else {
            return;
        };
        if old.cloud_folder != new.cloud_folder {
            self.watched = None;
            self.watcher = None;
            self.handle(CloudJob::Startup);
            return;
        }
        if !layout.exists() {
            return;
        }
        let device = self.device();
        let path = layout.settings_file(&device.id);
        let mut doc = read_json::<KeyedDoc>(&path).unwrap_or_else(|| KeyedDoc {
            device_id: device.id.0.clone(),
            ..KeyedDoc::default()
        });
        stamp_changes(&mut doc, &synced_view(old), &synced_view(new), unix_now());
        if let Err(error) = write_json_atomic(&path, &doc) {
            tracing::warn!("cannot save cloud settings: {error}");
        }
    }

    fn context<'a>(
        &'a self,
        layout: &'a CloudLayout,
        device: &'a LocalDevice,
        store: &'a Store,
    ) -> CloudContext<'a> {
        CloudContext {
            layout,
            device,
            store,
            cache: &self.cache,
            now: unix_now(),
        }
    }

    fn with_target(
        &mut self,
        layout: &CloudLayout,
        app_id: u32,
        work: impl FnOnce(&CloudContext, &GameTarget) -> CloudResult<SyncEvent>,
    ) -> CloudResult<()> {
        if !self.ready() {
            return Ok(());
        }
        let running: BTreeSet<u32> = self.state().running().keys().copied().collect();
        if skip_for_sync(app_id, &running, &self.state().launching()) {
            self.state().cloud.set_error(Some("gameRunning".into()));
            return Ok(());
        }
        let inv = inventory(&self.state());
        let Ok(info) = game_target(&self.state(), &inv, app_id) else {
            return Ok(());
        };
        let Some(store) = &self.store else {
            return Ok(());
        };
        let device = self.device();
        let lock = self
            .state()
            .cloud
            .game_lock(&info.target.key.as_folder_key());
        let _held = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let event = work(&self.context(layout, &device, store), &info.target)?;
        self.announce(&info.target, &event);
        Ok(())
    }

    fn reconcile_all(&mut self, layout: &CloudLayout) -> CloudResult<()> {
        if !self.ready() {
            return Ok(());
        }
        let state = self.state();
        let inv = inventory(&state);
        let running: BTreeSet<u32> = state.running().keys().copied().collect();
        let launching = state.launching().clone();
        let known: Vec<u32> = state
            .store()
            .games_overview()
            .unwrap_or_default()
            .into_iter()
            .map(|g| g.app_id)
            .collect();
        let mut targets: Vec<GameTarget> = known
            .iter()
            .filter(|id| !skip_for_sync(**id, &running, &launching))
            .filter_map(|id| game_target(&state, &inv, *id).ok())
            .filter(|info| info.enabled && !info.target.roots.is_empty())
            .map(|info| info.target)
            .collect();
        let local_keys: BTreeSet<CloudKey> = targets.iter().map(|t| t.key.clone()).collect();
        for (key, _) in layout.games() {
            if let CloudKey::Steam(app_id) = key
                && !local_keys.contains(&key)
                && !known.contains(&app_id)
                && !skip_for_sync(app_id, &running, &launching)
                && let Some(target) = cloud_only_target(&state, &inv, layout, app_id)
            {
                targets.push(target);
            }
        }
        let Some(store) = &self.store else {
            return Ok(());
        };
        let device = self.device();
        for target in targets {
            let lock = self.state().cloud.game_lock(&target.key.as_folder_key());
            let _held = lock.lock().unwrap_or_else(PoisonError::into_inner);
            if self.state().launching().contains(&target.app_id) {
                continue;
            }
            let ctx = self.context(layout, &device, store);
            match reconcile(&ctx, &target, ctx.now) {
                Ok(event) => self.announce(&target, &event),
                Err(CloudError::MissingRoot) => return Err(CloudError::MissingRoot),
                Err(error) => tracing::warn!("cloud sync of {}: {error}", target.name),
            }
        }
        Ok(())
    }

    fn game_started(&mut self, layout: &CloudLayout, app_id: u32) -> CloudResult<()> {
        let state = self.state();
        let inv = inventory(&state);
        let Ok(info) = game_target(&state, &inv, app_id) else {
            return Ok(());
        };
        if !info.enabled || info.target.roots.is_empty() {
            return Ok(());
        }
        let device = self.device();
        let game = layout.game_dir_for_write(&info.target.key, &info.target.name)?;
        let now = unix_now();
        write_lock(
            &game,
            &device.id,
            &LockRecord {
                device_id: device.id.0.clone(),
                device_name: device.name.clone(),
                game_key: info.target.key.as_folder_key(),
                started_at: now,
                renewed_at: now,
            },
        )?;
        Ok(())
    }

    fn game_exited(&mut self, layout: &CloudLayout, app_id: u32) -> CloudResult<()> {
        let syncs = |info: &TargetInfo| info.enabled && !info.target.roots.is_empty();
        if game_target(&self.state(), &inventory(&self.state()), app_id).is_ok_and(|i| syncs(&i)) {
            std::thread::sleep(SETTLE);
        }
        let inv = inventory(&self.state());
        let Ok(info) = game_target(&self.state(), &inv, app_id) else {
            return Ok(());
        };
        let device = self.device();
        let mut synced = Ok(());
        if syncs(&info)
            && self.ready()
            && let Some(store) = &self.store
        {
            let lock = self
                .state()
                .cloud
                .game_lock(&info.target.key.as_folder_key());
            let _held = lock.lock().unwrap_or_else(PoisonError::into_inner);
            let ctx = self.context(layout, &device, store);
            synced = reconcile(&ctx, &info.target, ctx.now)
                .map(|event| self.announce(&info.target, &event));
        }
        for game in layout.game_dirs(&info.target.key) {
            remove_own_lock(&game, &device.id)?;
        }
        synced
    }

    fn tick(&mut self) {
        let Some(layout) = self.layout().filter(CloudLayout::exists) else {
            return;
        };
        let now = unix_now();
        if now - self.last_renew >= LOCK_RENEW_SECS {
            self.last_renew = now;
            self.renew_locks(&layout, now);
        }
        self.handle(CloudJob::ReconcileAll);
    }

    fn renew_locks(&self, layout: &CloudLayout, now: i64) {
        let device = self.device();
        let running: Vec<u32> = self.state().running().keys().copied().collect();
        let inv = inventory(&self.state());
        for app_id in running {
            let Ok(info) = game_target(&self.state(), &inv, app_id) else {
                continue;
            };
            for game in layout.game_dirs(&info.target.key) {
                if let Some(mut record) = own_lock(&game, &device.id) {
                    record.renewed_at = now;
                    if let Err(error) = write_lock(&game, &device.id, &record) {
                        tracing::warn!("cannot renew the cloud lock: {error}");
                    }
                }
            }
        }
    }

    fn announce(&self, target: &GameTarget, event: &SyncEvent) {
        announce(&self.app, target, event);
    }
}

pub fn needs_rescan(event: &SyncEvent) -> bool {
    matches!(
        event,
        SyncEvent::Restored { .. } | SyncEvent::Branched { .. }
    )
}

pub fn announce(app: &AppHandle, target: &GameTarget, event: &SyncEvent) {
    if matches!(event, SyncEvent::Nothing | SyncEvent::Waiting) {
        return;
    }
    if needs_rescan(event) {
        app.state::<AppState>()
            .with_watcher(|w| w.request_full_sync());
        let _ = app.emit(GAMES_CHANGED, ());
    }
    let _ = app.emit(CLOUD_CHANGED, Some(target.app_id));
}

pub fn set_game_pref(
    settings: &mut Settings,
    key: &CloudKey,
    change: impl FnOnce(&mut GameSyncPref),
) {
    let pref = settings.game_sync.entry(key.as_folder_key()).or_default();
    change(pref);
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn restores_and_branches_rescan_the_game() {
        let restored = SyncEvent::Restored {
            snapshot: "a".into(),
            playtime_seconds: None,
            kept_local: None,
        };
        assert!(needs_rescan(&restored));
        assert!(!needs_rescan(&SyncEvent::Pushed {
            snapshot: "a".into()
        }));
        assert!(!needs_rescan(&SyncEvent::Nothing));
    }

    #[test]
    fn launching_games_are_skipped_by_the_worker() {
        let none_running = BTreeSet::new();
        let none_launching = HashSet::new();
        assert!(skip_for_sync(7, &none_running, &HashSet::from([7])));
        assert!(skip_for_sync(7, &BTreeSet::from([7]), &none_launching));
        assert!(!skip_for_sync(7, &none_running, &none_launching));
    }

    #[test]
    fn sync_disabled_without_cloud_folder() {
        assert!(cloud_layout(&Settings::default()).is_none());
        let settings = Settings {
            cloud_folder: Some(PathBuf::from("D:/OneDrive/AchievementWatcher")),
            ..Settings::default()
        };
        assert_eq!(
            cloud_layout(&settings).unwrap().root(),
            Path::new("D:/OneDrive/AchievementWatcher")
        );
    }

    #[test]
    fn manual_path_comes_from_game_sync() {
        let mut settings = Settings::default();
        settings.game_sync.insert(
            "374320".into(),
            GameSyncPref {
                enabled: None,
                save_path: Some("<winDocuments>/My DS3".into()),
            },
        );
        assert_eq!(
            manual_path(&settings, &CloudKey::Steam(374320))
                .unwrap()
                .as_str(),
            "<winDocuments>/My DS3"
        );
        assert_eq!(manual_path(&settings, &CloudKey::Steam(1)), None);
    }

    #[test]
    fn user_choice_comes_from_game_sync() {
        let mut settings = Settings::default();
        settings.game_sync.insert(
            "7".into(),
            GameSyncPref {
                enabled: Some(false),
                save_path: None,
            },
        );
        assert_eq!(user_choice(&settings, &CloudKey::Steam(7)), Some(false));
        assert_eq!(user_choice(&settings, &CloudKey::Steam(8)), None);
    }

    #[test]
    fn only_save_only_snapshots_are_pre_restored() {
        use aw_core::cloud::snapshot::Contents;
        let steam = Contents {
            save: true,
            emulator_files: false,
            state: false,
        };
        let emulator = Contents {
            save: true,
            emulator_files: true,
            state: true,
        };
        assert!(cloud_only_allowed(&steam));
        assert!(!cloud_only_allowed(&emulator));
    }

    #[test]
    fn sync_waits_for_detection_and_the_save_list() {
        assert!(!inputs_ready(None, ManifestState::Ready));
        assert!(!inputs_ready(Some(1), ManifestState::Pending));
        assert!(inputs_ready(Some(1), ManifestState::Ready));
        assert!(inputs_ready(Some(1), ManifestState::Unavailable));
    }
}
