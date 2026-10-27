use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use aw_core::cloud::capture::HashCache;
use aw_core::cloud::device::{DeviceRecord, LocalDevice};
use aw_core::cloud::identity::{push_identities, restore_identities};
use aw_core::cloud::key::{CloudKey, GameKind, cloud_key_for, game_kind};
use aw_core::cloud::keyed::{self, KeyedDoc, stamp_changes};
use aw_core::cloud::layout::{CloudLayout, read_json, write_json_atomic};
use aw_core::cloud::lock::{LOCK_RENEW_SECS, own_lock, write_lock};
use aw_core::cloud::manifest::{load_index, refresh};
use aw_core::cloud::migrate::adopt_former_ids;
use aw_core::cloud::steamcloud::fetch_has_cloud;
use aw_core::cloud::sync::{CloudContext, GameTarget, SyncEvent, reconcile, set_principal, undo};
use aw_core::cloud::{CloudError, CloudResult};
use aw_core::store::Store;
use aw_core::watcher::unix_now;
use notify::{RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};

use crate::app::{AppState, PLAY_CHANGED};
use crate::i18n::text;
use crate::settings::{Settings, synced_view};

use super::*;

const SETTLE: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_secs(60);
const DEBOUNCE: Duration = Duration::from_secs(2);
const DEBOUNCE_CAP: Duration = Duration::from_secs(10);
const WATCH_REFRESH_TICKS: u32 = 10;
const MANIFEST_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const STEAM_LOOKUP_SPACING: Duration = Duration::from_secs(2);
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(15);
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(120);
const README_KEY: &str = "cloud.readme";
const INDEX_FILE: &str = "ludusavi-index.json";

struct Worker {
    app: AppHandle,
    jobs: Sender<CloudJob>,
    store: Option<Store>,
    cache: Arc<HashCache>,
    watcher: Option<notify::RecommendedWatcher>,
    watched: Option<PathBuf>,
    started: Option<PathBuf>,
    background: bool,
    last_renew: i64,
    ticks: u32,
}

pub fn run_cloud_worker(app: AppHandle, jobs: Receiver<CloudJob>, sender: Sender<CloudJob>) {
    let store = Store::open(&app.state::<AppState>().paths.db_file)
        .map_err(|error| tracing::error!("cloud worker cannot open the database: {error}"))
        .ok();
    let cache = Arc::clone(&app.state::<AppState>().cloud.cache);
    let mut worker = Worker {
        app,
        jobs: sender,
        store,
        cache,
        watcher: None,
        watched: None,
        started: None,
        background: false,
        last_renew: 0,
        ticks: 0,
    };
    let mut next_tick = Instant::now() + TICK;
    loop {
        match jobs.recv_timeout(next_tick.saturating_duration_since(Instant::now())) {
            Ok(CloudJob::CloudFilesChanged(first)) => {
                let mut paths = first;
                let until = Instant::now() + DEBOUNCE_CAP;
                while Instant::now() < until {
                    let Ok(job) = jobs.recv_timeout(DEBOUNCE) else {
                        break;
                    };
                    match job {
                        CloudJob::CloudFilesChanged(more) => paths.extend(more),
                        other => worker.handle(other),
                    }
                }
                worker.handle(CloudJob::CloudFilesChanged(paths));
            }
            Ok(job) => worker.handle(job),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if Instant::now() >= next_tick {
            worker.tick();
            next_tick = Instant::now() + TICK;
        }
    }
}

fn best_effort<T, E: std::fmt::Display>(what: &str, result: Result<T, E>) {
    if let Err(error) = result {
        tracing::warn!("cloud startup, {what}: {error}");
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
        local_device(&self.state())
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
        let before = self.state().cloud.last_error();
        if !layout.exists() {
            self.state().cloud.set_error(Some("missingRoot".into()));
            self.started = None;
            self.watched = None;
            self.watcher = None;
            if before.as_deref() != Some("missingRoot") {
                self.emit(None);
            }
            return;
        }
        let global = matches!(job, CloudJob::Startup);
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
            CloudJob::ReconcileAll => self.reconcile_all(&layout, None),
            CloudJob::CloudFilesChanged(paths) => match layout.changed_games(&paths) {
                Some(keys) => self.reconcile_all(&layout, Some(&keys)),
                None => {
                    self.merge_settings(&layout);
                    self.reconcile_all(&layout, None)
                }
            },
            CloudJob::Undo(app_id) => self.with_target(&layout, app_id, undo),
            CloudJob::SetPrincipal(app_id, id) => {
                self.with_target(&layout, app_id, |ctx, target| {
                    set_principal(ctx, target, &id, aw_core::cloud::heads::HeadReason::Manual)
                })
            }
            CloudJob::SyncGame(app_id) => self.with_target(&layout, app_id, |ctx, target| {
                reconcile(ctx, target, ctx.now)
            }),
            CloudJob::SettingsChanged { .. } => Ok(()),
        };
        if let Err(error) = result {
            tracing::warn!("cloud sync: {error}");
            self.state().cloud.set_error(Some(error_code(&error)));
        }
        if global || self.state().cloud.last_error() != before {
            self.emit(None);
        }
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
        let device = self.device();
        let now = unix_now();
        let locale = self.state().ui_locale();
        best_effort("readme", layout.write_readme(&text(locale, README_KEY)));
        best_effort("former device ids", adopt_former_ids(layout, &device));
        best_effort(
            "device file",
            write_json_atomic(
                &layout.device_file(&device.id),
                &DeviceRecord {
                    name: device.name.clone(),
                    last_seen: now,
                },
            ),
        );
        best_effort("settings", self.ensure_settings_doc(layout, &device, now));
        self.merge_settings(layout);
        if let Some(store) = &self.store {
            let folders = self.state().folders.clone();
            if !store.has_any_cloud_base().unwrap_or(true) {
                best_effort(
                    "identities",
                    restore_identities(layout, &folders, store, &device.name, now),
                );
            }
            best_effort(
                "identities",
                push_identities(layout, &folders, store, &device.name, now),
            );
        }
        self.watch(layout);
        if !self.background {
            self.background = true;
            self.spawn_manifest_refresh();
            self.spawn_steam_cloud_lookups();
        }
        self.started = Some(layout.root().to_path_buf());
        self.reconcile_all(layout, None)
    }

    fn watch(&mut self, layout: &CloudLayout) {
        if self.watched.as_deref() == Some(layout.root()) {
            return;
        }
        let sender = self.jobs.clone();
        let last_change = Arc::clone(&self.state().cloud.last_change);
        let root = layout.root().to_path_buf();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if let Ok(event) = event {
                *last_change.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
                let paths = if event.paths.is_empty() {
                    vec![root.clone()]
                } else {
                    event.paths
                };
                let _ = sender.send(CloudJob::CloudFilesChanged(paths));
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

    fn busy_here(&self, app_id: u32) -> bool {
        let state = self.state();
        state.running().contains_key(&app_id) || state.launching().contains(&app_id)
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
        let ctx = self.context(layout, &device, store);
        let event = tracked(&self.app, &info.target, || work(&ctx, &info.target))?;
        self.announce(&info.target, &event);
        Ok(())
    }

    fn reconcile_all(
        &mut self,
        layout: &CloudLayout,
        only: Option<&BTreeSet<CloudKey>>,
    ) -> CloudResult<()> {
        if !self.ready() || only.is_some_and(BTreeSet::is_empty) {
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
        let wanted = |key: Option<CloudKey>| {
            only.is_none_or(|keys| key.is_some_and(|key| keys.contains(&key)))
        };
        let mut targets: Vec<GameTarget> = known
            .iter()
            .filter(|id| !skip_for_sync(**id, &running, &launching))
            .filter(|id| wanted(cloud_key_for(**id, &inv.found)))
            .filter_map(|id| game_target(&state, &inv, *id).ok())
            .filter(|info| info.enabled && !info.target.roots.is_empty())
            .map(|info| info.target)
            .collect();
        let local_keys: BTreeSet<CloudKey> = targets.iter().map(|t| t.key.clone()).collect();
        for (key, _) in layout.games() {
            if let CloudKey::Steam(app_id) = key
                && wanted(Some(key.clone()))
                && !local_keys.contains(&key)
                && !known.contains(&app_id)
                && !skip_for_sync(app_id, &running, &launching)
                && let Some(target) = cloud_only_target(&state, &inv, layout, app_id)
            {
                targets.push(target);
            }
        }
        targets.retain(|target| only.is_none_or(|keys| keys.contains(&target.key)));
        let Some(store) = &self.store else {
            return Ok(());
        };
        let device = self.device();
        for target in targets {
            let lock = self.state().cloud.game_lock(&target.key.as_folder_key());
            let _held = lock.lock().unwrap_or_else(PoisonError::into_inner);
            if self.busy_here(target.app_id) {
                continue;
            }
            let ctx = self.context(layout, &device, store);
            match tracked(&self.app, &target, || reconcile(&ctx, &target, ctx.now)) {
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
        mark_open(layout, &info.target, &self.device())?;
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
            if self.busy_here(app_id) {
                return Ok(());
            }
            let ctx = self.context(layout, &device, store);
            synced = tracked(&self.app, &info.target, || {
                reconcile(&ctx, &info.target, ctx.now)
            })
            .map(|event| self.announce(&info.target, &event));
        }
        if self.busy_here(app_id) {
            return synced;
        }
        release_lock(layout, &device.id, &info.target.key)?;
        synced
    }

    fn tick(&mut self) {
        let Some(layout) = self.layout().filter(CloudLayout::exists) else {
            return;
        };
        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks % WATCH_REFRESH_TICKS == 0 {
            self.watcher = None;
            self.watched = None;
        }
        if self.started.is_some() {
            self.watch(&layout);
        }
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
