use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use aw_core::cloud::device::LocalDevice;
use aw_core::cloud::key::{CloudKey, GameKind};
use aw_core::cloud::layout::CloudLayout;
use aw_core::cloud::lock::{LockRecord, foreign_locks};
use aw_core::cloud::provider::{DriveWait, ProviderHints, drive_wait, provider_for};
use aw_core::cloud::sync::{CloudContext, GameTarget, SyncEvent, reconcile_cancellable};
use aw_core::cloud::{CancelGate, CloudError, CloudResult};
use aw_core::discovery::launch::suggest_exe;
use aw_core::discovery::running::{process_started_at, shell_open, terminate};
use aw_core::store::Store;
use aw_core::watcher::unix_now;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app::{AppState, PLAY_CHANGED};
use crate::cloud::{
    TargetInfo, announce, cloud_layout, game_target, inputs_ready, inventory, local_device,
    mark_open, release_lock, tracked,
};
use crate::games::launch_exe_key;

const STUCK_AFTER: Duration = Duration::from_secs(10);
const RETRY: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(500);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(60);
const ELEVATION_REQUIRED: i32 = 740;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlayState {
    #[serde(rename_all = "camelCase")]
    Idle {
        needs_exe: bool,
    },
    Syncing {
        stuck: bool,
    },
    Launching,
    Running,
    Exiting,
    Failed {
        reason: String,
    },
    OpenElsewhere {
        device: String,
        since: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlayResponse {
    Started,
    OpenElsewhere { device: String, since: i64 },
    NeedsExe,
    AlreadyRunning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowStage {
    Syncing { stuck: bool },
    Launching,
    Failed(String),
    Elsewhere { device: String, since: i64 },
}

#[derive(Clone)]
pub struct PlayFlow {
    pub stage: FlowStage,
    pub gate: Arc<CancelGate>,
    pub anyway: Arc<AtomicBool>,
}

impl PlayFlow {
    pub fn new() -> PlayFlow {
        PlayFlow {
            stage: FlowStage::Syncing { stuck: false },
            gate: Arc::new(CancelGate::default()),
            anyway: Arc::new(AtomicBool::new(false)),
        }
    }

    fn active(&self) -> bool {
        matches!(self.stage, FlowStage::Syncing { .. } | FlowStage::Launching)
    }

    fn decision(&self) -> Option<Choice> {
        if !self.gate.is_cancelled() {
            return None;
        }
        Some(if self.anyway.load(Ordering::SeqCst) {
            Choice::Go
        } else {
            Choice::Cancel
        })
    }
}

enum Launch {
    Steam,
    Exe(PathBuf),
}

#[derive(Clone)]
struct SyncTarget {
    layout: CloudLayout,
    target: GameTarget,
}

#[derive(Debug, PartialEq, Eq)]
enum Choice {
    Go,
    Cancel,
}

#[derive(Debug, PartialEq, Eq)]
enum Step {
    Go,
    Cancel,
    AlreadyOpen,
    Wait,
    Retry { stuck: bool },
}

enum Prepared {
    Go(Option<Box<SyncTarget>>),
    Cancel,
    AlreadyOpen,
    Elsewhere { device: String, since: i64 },
}

fn next_step(
    result: Option<&CloudResult<SyncEvent>>,
    decision: Option<Choice>,
    running: bool,
) -> Step {
    if running {
        return Step::AlreadyOpen;
    }
    match (decision, result) {
        (Some(Choice::Cancel), _) => Step::Cancel,
        (Some(Choice::Go), _) => Step::Go,
        (None, None) => Step::Wait,
        (None, Some(Ok(SyncEvent::Waiting) | Err(CloudError::Cancelled))) => {
            Step::Retry { stuck: false }
        }
        (None, Some(Ok(_))) => Step::Go,
        (None, Some(Err(_))) => Step::Retry { stuck: true },
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Gate {
    Skip,
    Wait { stuck: bool },
    SyncTarget,
}

fn sync_gate(folder_exists: bool, inputs_ready: bool, target: Option<(bool, bool)>) -> Gate {
    if !folder_exists {
        return Gate::Wait { stuck: true };
    }
    if !inputs_ready {
        return Gate::Wait { stuck: false };
    }
    match target {
        Some((true, true)) => Gate::SyncTarget,
        _ => Gate::Skip,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum LaunchPick {
    Steam,
    Exe(PathBuf),
    Guessed(PathBuf),
    NeedsExe,
}

fn pick_launch(
    steam_original: bool,
    chosen: Option<PathBuf>,
    remembered: Option<PathBuf>,
    guess: Option<PathBuf>,
) -> LaunchPick {
    if let Some(exe) = chosen {
        return LaunchPick::Exe(exe);
    }
    if steam_original {
        return LaunchPick::Steam;
    }
    match (remembered, guess) {
        (Some(exe), _) => LaunchPick::Exe(exe),
        (None, Some(exe)) => LaunchPick::Guessed(exe),
        (None, None) => LaunchPick::NeedsExe,
    }
}

fn chosen_exe_key(app_id: u32) -> String {
    format!("launch_exe_chosen:{app_id}")
}

fn existing_exe(state: &AppState, key: &str) -> Option<PathBuf> {
    let stored = state.store().meta_get(key).ok().flatten()?;
    Some(PathBuf::from(stored)).filter(|path| path.is_file())
}

fn launch_pick(state: &AppState, app_id: u32) -> LaunchPick {
    let inv = inventory(state);
    pick_launch(
        inv.kind_of(state, app_id) == GameKind::SteamOriginal,
        existing_exe(state, &chosen_exe_key(app_id)),
        existing_exe(state, &launch_exe_key(app_id)),
        inv.game_dir(app_id).and_then(|dir| suggest_exe(&dir)),
    )
}

fn needs_elevation(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(ELEVATION_REQUIRED)
}

pub fn derive_state(
    flow: Option<&PlayFlow>,
    running: bool,
    exiting: bool,
    has_exe_or_steam: bool,
) -> PlayState {
    if running {
        return PlayState::Running;
    }
    if exiting {
        return PlayState::Exiting;
    }
    match flow.map(|flow| &flow.stage) {
        Some(FlowStage::Syncing { stuck }) => PlayState::Syncing { stuck: *stuck },
        Some(FlowStage::Launching) => PlayState::Launching,
        Some(FlowStage::Failed(reason)) => PlayState::Failed {
            reason: reason.clone(),
        },
        Some(FlowStage::Elsewhere { device, since }) => PlayState::OpenElsewhere {
            device: device.clone(),
            since: *since,
        },
        None => PlayState::Idle {
            needs_exe: !has_exe_or_steam,
        },
    }
}

pub fn begin_flow(plays: &mut HashMap<u32, PlayFlow>, app_id: u32) -> bool {
    if plays.get(&app_id).is_some_and(PlayFlow::active) {
        return false;
    }
    plays.insert(app_id, PlayFlow::new());
    true
}

fn device(state: &AppState) -> LocalDevice {
    local_device(state)
}

fn is_running(app: &AppHandle, app_id: u32) -> bool {
    app.state::<AppState>().running().contains_key(&app_id)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchChoice {
    pub exe: Option<String>,
    pub chosen: bool,
    pub steam: bool,
}

fn can_launch(state: &AppState, app_id: u32) -> bool {
    launch_pick(state, app_id) != LaunchPick::NeedsExe
}

#[tauri::command]
pub async fn launch_choice(
    state: State<'_, AppState>,
    app_id: u32,
) -> Result<LaunchChoice, String> {
    let chosen = existing_exe(&state, &chosen_exe_key(app_id)).is_some();
    Ok(match launch_pick(&state, app_id) {
        LaunchPick::Steam => LaunchChoice {
            exe: None,
            chosen,
            steam: true,
        },
        LaunchPick::Exe(exe) | LaunchPick::Guessed(exe) => LaunchChoice {
            exe: Some(exe.display().to_string()),
            chosen,
            steam: false,
        },
        LaunchPick::NeedsExe => LaunchChoice {
            exe: None,
            chosen,
            steam: false,
        },
    })
}

fn foreign_lock(state: &AppState, layout: &CloudLayout, key: &CloudKey) -> Option<LockRecord> {
    foreign_locks(&layout.game_dirs(key), &device(state), unix_now())
        .into_iter()
        .max_by_key(|lock| lock.started_at)
}

fn set_stage(app: &AppHandle, app_id: u32, stage: Option<FlowStage>) {
    let state = app.state::<AppState>();
    {
        let mut plays = state.plays();
        match stage {
            Some(stage) => {
                if let Some(flow) = plays.get_mut(&app_id) {
                    flow.stage = stage;
                }
            }
            None => {
                plays.remove(&app_id);
            }
        }
    }
    let _ = app.emit(PLAY_CHANGED, app_id);
}

fn finish(app: &AppHandle, app_id: u32, stage: Option<FlowStage>) {
    app.state::<AppState>().launching().remove(&app_id);
    set_stage(app, app_id, stage);
}

#[tauri::command]
pub async fn play_state(state: State<'_, AppState>, app_id: u32) -> Result<PlayState, String> {
    let running = state.running().contains_key(&app_id);
    let exiting = state.exiting().contains(&app_id);
    let flow = state.plays().get(&app_id).cloned();
    Ok(derive_state(
        flow.as_ref(),
        running,
        exiting,
        can_launch(&state, app_id),
    ))
}

#[tauri::command]
pub async fn play_game(
    app: AppHandle,
    state: State<'_, AppState>,
    app_id: u32,
    confirmed_elsewhere: bool,
) -> Result<PlayResponse, String> {
    if state.running().contains_key(&app_id) {
        return Ok(PlayResponse::AlreadyRunning);
    }
    if state.plays().get(&app_id).is_some_and(PlayFlow::active) {
        return Ok(PlayResponse::Started);
    }
    let launch = match launch_pick(&state, app_id) {
        LaunchPick::Steam => Launch::Steam,
        LaunchPick::Exe(exe) => Launch::Exe(exe),
        LaunchPick::Guessed(exe) => {
            let remembered = state
                .store()
                .meta_set(&launch_exe_key(app_id), &exe.display().to_string());
            if let Err(error) = remembered {
                tracing::warn!("cannot remember the exe of {app_id}: {error}");
            }
            Launch::Exe(exe)
        }
        LaunchPick::NeedsExe => return Ok(PlayResponse::NeedsExe),
    };
    let inv = inventory(&state);
    let layout = cloud_layout(&state.settings_snapshot());
    let target = game_target(&state, &inv, app_id).ok();
    let wanted = layout.is_some() && target.as_ref().is_none_or(|info| info.enabled);
    let mut checked = confirmed_elsewhere;
    if wanted
        && !checked
        && let (Some(layout), Some(info)) = (layout.as_ref(), target.as_ref())
        && layout.exists()
    {
        checked = true;
        if let Some(lock) = foreign_lock(&state, layout, &info.target.key) {
            return Ok(PlayResponse::OpenElsewhere {
                device: lock.device_name,
                since: lock.started_at,
            });
        }
    }
    if !begin_flow(&mut state.plays(), app_id) {
        return Ok(PlayResponse::Started);
    }
    state.launching().insert(app_id);
    let _ = app.emit(PLAY_CHANGED, app_id);
    let handle = app.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("aw-play-{app_id}"))
        .spawn(move || run_flow(&handle, app_id, wanted, checked, &launch));
    if let Err(error) = spawned {
        finish(&app, app_id, None);
        return Err(error.to_string());
    }
    Ok(PlayResponse::Started)
}

#[tauri::command]
pub async fn play_anyway(state: State<'_, AppState>, app_id: u32) -> Result<(), String> {
    let flow = state.plays().get(&app_id).cloned();
    if let Some(flow) = flow {
        flow.anyway.store(true, Ordering::SeqCst);
        flow.gate.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_play(
    app: AppHandle,
    state: State<'_, AppState>,
    app_id: u32,
) -> Result<(), String> {
    let flow = state.plays().get(&app_id).cloned();
    match flow {
        Some(flow) if flow.active() => flow.gate.cancel(),
        Some(_) => {
            state.plays().remove(&app_id);
            let _ = app.emit(PLAY_CHANGED, app_id);
        }
        None => {}
    }
    Ok(())
}

#[tauri::command]
pub fn stop_game(state: State<'_, AppState>, app_id: u32) -> Result<(), String> {
    for pid in state.pids_of(app_id) {
        terminate(pid).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_launch_exe(
    app: AppHandle,
    state: State<'_, AppState>,
    app_id: u32,
    path: Option<String>,
) -> Result<(), String> {
    if let Some(path) = &path {
        let exe = Path::new(path);
        let is_exe = exe
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
        if !is_exe || !exe.is_file() {
            return Err("notAnExe".into());
        }
    }
    state
        .store()
        .meta_set(&chosen_exe_key(app_id), path.as_deref().unwrap_or_default())
        .map_err(|error| error.to_string())?;
    let _ = app.emit(PLAY_CHANGED, app_id);
    Ok(())
}

fn mark_stuck(app: &AppHandle, app_id: u32, stuck: &mut bool) {
    if !*stuck {
        *stuck = true;
        set_stage(app, app_id, Some(FlowStage::Syncing { stuck: true }));
    }
}

fn pause(
    app: &AppHandle,
    app_id: u32,
    flow: &PlayFlow,
    duration: Duration,
    started: Instant,
    stuck: &mut bool,
) -> Option<Step> {
    let until = Instant::now() + duration;
    loop {
        match next_step(None, flow.decision(), is_running(app, app_id)) {
            Step::Wait => {}
            step => return Some(step),
        }
        if started.elapsed() >= STUCK_AFTER {
            mark_stuck(app, app_id, stuck);
        }
        if Instant::now() >= until {
            return None;
        }
        std::thread::sleep(POLL);
    }
}

fn wait_for_drive(
    app: &AppHandle,
    app_id: u32,
    flow: &PlayFlow,
    sync: &SyncTarget,
    started: Instant,
    stuck: &mut bool,
) -> Option<Step> {
    let provider = provider_for(sync.layout.root(), &ProviderHints::from_env())?;
    let waiting = Instant::now();
    loop {
        let client_age = process_started_at(provider.client_exe())
            .and_then(|at| SystemTime::now().duration_since(at).ok());
        let quiet = app
            .state::<AppState>()
            .cloud
            .quiet_for()
            .min(waiting.elapsed());
        match drive_wait(client_age, quiet, waiting.elapsed()) {
            DriveWait::Ready => return None,
            DriveWait::ClientClosed => mark_stuck(app, app_id, stuck),
            DriveWait::Wait => {}
        }
        if let Some(step) = pause(app, app_id, flow, POLL, started, stuck) {
            return Some(step);
        }
    }
}

fn sync_once(
    app: &AppHandle,
    sync: &SyncTarget,
    app_id: u32,
    gate: &CancelGate,
) -> CloudResult<SyncEvent> {
    let state = app.state::<AppState>();
    let store = Store::open(&state.paths.db_file)?;
    let device = device(&state);
    let lock = state.cloud.game_lock(&sync.target.key.as_folder_key());
    let _held = lock.lock().unwrap_or_else(PoisonError::into_inner);
    if state.running().contains_key(&app_id) {
        return Ok(SyncEvent::Nothing);
    }
    let now = unix_now();
    let ctx = CloudContext {
        layout: &sync.layout,
        device: &device,
        store: &store,
        cache: &state.cloud.cache,
        now,
    };
    tracked(app, &sync.target, || {
        reconcile_cancellable(&ctx, &sync.target, now, gate)
    })
}

fn attempt(
    app: &AppHandle,
    app_id: u32,
    flow: &PlayFlow,
    sync: &SyncTarget,
    started: Instant,
    stuck: &mut bool,
) -> Step {
    let (tx, rx) = std::sync::mpsc::channel();
    let (handle, gate, job) = (app.clone(), Arc::clone(&flow.gate), sync.clone());
    std::thread::spawn(move || {
        let _ = tx.send(sync_once(&handle, &job, app_id, &gate));
    });
    loop {
        let result = match rx.recv_timeout(POLL) {
            Ok(result) => Some(result),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Err(CloudError::Cancelled)),
        };
        if let Some(Ok(event)) = &result {
            tracing::info!("play {app_id}: synced before launch: {event:?}");
            announce(app, &sync.target, event);
        }
        if let Some(Err(error)) = &result {
            tracing::warn!("play {app_id}: sync before launch: {error}");
        }
        match next_step(result.as_ref(), flow.decision(), is_running(app, app_id)) {
            Step::Wait => {
                if started.elapsed() >= STUCK_AFTER {
                    mark_stuck(app, app_id, stuck);
                }
            }
            step => {
                if result.is_none() {
                    flow.gate.cancel();
                }
                return step;
            }
        }
    }
}

fn prepare(app: &AppHandle, app_id: u32, flow: &PlayFlow, mut checked: bool) -> Prepared {
    let started = Instant::now();
    let mut stuck = false;
    let mut drive_ready = false;
    loop {
        let state = app.state::<AppState>();
        let Some(layout) = cloud_layout(&state.settings_snapshot()) else {
            return Prepared::Go(None);
        };
        let last_sync = state.with_watcher(|w| w.snapshot().last_sync_at).flatten();
        let ready = inputs_ready(last_sync, state.cloud.manifest_state());
        let info: Option<TargetInfo> = if ready {
            game_target(&state, &inventory(&state), app_id).ok()
        } else {
            None
        };
        let shape = info
            .as_ref()
            .map(|info| (info.enabled, !info.target.roots.is_empty()));
        let step = match (sync_gate(layout.exists(), ready, shape), info) {
            (Gate::Skip, _) => return Prepared::Go(None),
            (Gate::Wait { stuck: now }, _) => {
                if now {
                    mark_stuck(app, app_id, &mut stuck);
                }
                pause(app, app_id, flow, POLL, started, &mut stuck)
            }
            (Gate::SyncTarget, None) => return Prepared::Go(None),
            (Gate::SyncTarget, Some(info)) => {
                let sync = SyncTarget {
                    layout,
                    target: info.target,
                };
                if !checked {
                    checked = true;
                    if let Some(lock) = foreign_lock(&state, &sync.layout, &sync.target.key) {
                        return Prepared::Elsewhere {
                            device: lock.device_name,
                            since: lock.started_at,
                        };
                    }
                }
                let drive = if drive_ready {
                    None
                } else {
                    wait_for_drive(app, app_id, flow, &sync, started, &mut stuck)
                };
                drive_ready = drive.is_none();
                match drive
                    .unwrap_or_else(|| attempt(app, app_id, flow, &sync, started, &mut stuck))
                {
                    Step::Go => return Prepared::Go(Some(Box::new(sync))),
                    Step::Retry { stuck: now } => {
                        if now {
                            mark_stuck(app, app_id, &mut stuck);
                        }
                        pause(app, app_id, flow, RETRY, started, &mut stuck)
                    }
                    step => Some(step),
                }
            }
        };
        match step {
            None | Some(Step::Wait | Step::Retry { .. }) => {}
            Some(Step::Go) => return Prepared::Go(None),
            Some(Step::Cancel) => return Prepared::Cancel,
            Some(Step::AlreadyOpen) => return Prepared::AlreadyOpen,
        }
    }
}

fn open_mark(app: &AppHandle, sync: &SyncTarget) {
    let device = device(&app.state::<AppState>());
    if let Err(error) = mark_open(&sync.layout, &sync.target, &device) {
        tracing::warn!("cannot mark {} as open: {error}", sync.target.name);
    }
}

fn release(app: &AppHandle, sync: Option<&SyncTarget>) {
    let Some(sync) = sync else { return };
    let device = device(&app.state::<AppState>());
    if let Err(error) = release_lock(&sync.layout, &device.id, &sync.target.key) {
        tracing::warn!(
            "cannot release the open mark of {}: {error}",
            sync.target.name
        );
    }
}

fn spawn_game(app_id: u32, launch: &Launch) -> std::io::Result<()> {
    match launch {
        Launch::Steam => std::process::Command::new("explorer")
            .arg(format!("steam://rungameid/{app_id}"))
            .spawn()
            .map(|_| ()),
        Launch::Exe(exe) => {
            let mut command = std::process::Command::new(exe);
            if let Some(dir) = exe.parent() {
                command.current_dir(dir);
            }
            match command.spawn() {
                Ok(_) => Ok(()),
                Err(error) if needs_elevation(&error) => shell_open(exe, exe.parent()),
                Err(error) => Err(error),
            }
        }
    }
}

fn run_flow(app: &AppHandle, app_id: u32, wanted: bool, checked: bool, launch: &Launch) {
    let Some(flow) = app.state::<AppState>().plays().get(&app_id).cloned() else {
        finish(app, app_id, None);
        return;
    };
    let sync = if wanted {
        match prepare(app, app_id, &flow, checked) {
            Prepared::Go(sync) => sync,
            Prepared::Cancel | Prepared::AlreadyOpen => {
                finish(app, app_id, None);
                return;
            }
            Prepared::Elsewhere { device, since } => {
                finish(app, app_id, Some(FlowStage::Elsewhere { device, since }));
                return;
            }
        }
    } else {
        None
    };
    if let Some(sync) = &sync {
        open_mark(app, sync);
    }
    set_stage(app, app_id, Some(FlowStage::Launching));
    if let Err(error) = spawn_game(app_id, launch) {
        tracing::warn!("play {app_id}: cannot start the game: {error}");
        release(app, sync.as_deref());
        finish(app, app_id, Some(FlowStage::Failed("launchFailed".into())));
        return;
    }
    let started = Instant::now();
    while started.elapsed() < LAUNCH_TIMEOUT {
        if is_running(app, app_id) {
            finish(app, app_id, None);
            return;
        }
        std::thread::sleep(POLL);
    }
    release(app, sync.as_deref());
    finish(
        app,
        app_id,
        Some(FlowStage::Failed("launchUnconfirmed".into())),
    );
}

#[cfg(test)]
mod tests {
    use aw_core::cloud::device::DeviceId;
    use aw_core::cloud::lock::{LockRecord, own_lock, write_lock};

    use super::*;

    #[test]
    fn running_wins_over_everything() {
        let flow = PlayFlow::new();
        assert_eq!(
            derive_state(Some(&flow), true, false, true),
            PlayState::Running
        );
    }

    #[test]
    fn exiting_shows_syncing_after_close() {
        assert_eq!(derive_state(None, false, true, true), PlayState::Exiting);
    }

    #[test]
    fn idle_without_exe_asks_for_one() {
        assert_eq!(
            derive_state(None, false, false, false),
            PlayState::Idle { needs_exe: true }
        );
        assert_eq!(
            derive_state(None, false, false, true),
            PlayState::Idle { needs_exe: false }
        );
    }

    #[test]
    fn active_and_failed_flows_show_their_stage() {
        let mut flow = PlayFlow::new();
        flow.stage = FlowStage::Syncing { stuck: true };
        assert_eq!(
            derive_state(Some(&flow), false, false, true),
            PlayState::Syncing { stuck: true }
        );
        flow.stage = FlowStage::Failed("launchUnconfirmed".into());
        assert_eq!(
            derive_state(Some(&flow), false, false, true),
            PlayState::Failed {
                reason: "launchUnconfirmed".into()
            }
        );
    }

    #[test]
    fn second_play_while_a_flow_runs_is_ignored() {
        let mut plays = HashMap::new();
        assert!(begin_flow(&mut plays, 7));
        assert!(!begin_flow(&mut plays, 7));
        assert_eq!(plays.len(), 1);
        plays.get_mut(&7).unwrap().stage = FlowStage::Failed("launchFailed".into());
        assert!(begin_flow(&mut plays, 7));
    }

    #[test]
    fn lock_is_removed_when_the_launch_never_starts() {
        let dir = tempfile::tempdir().unwrap();
        let layout = CloudLayout::new(dir.path().join("AW"));
        std::fs::create_dir_all(layout.root()).unwrap();
        let key = CloudKey::Steam(7);
        let device = DeviceId("desk".into());
        let game = layout.game_dir_for_write(&key, "Game").unwrap();
        write_lock(
            &game,
            &device,
            &LockRecord {
                device_id: "desk".into(),
                device_name: "Desktop".into(),
                game_key: "7".into(),
                started_at: 1,
                renewed_at: 1,
            },
        )
        .unwrap();
        release_lock(&layout, &device, &key).unwrap();
        assert!(own_lock(&game, &device).is_none());
    }

    fn ok(event: SyncEvent) -> CloudResult<SyncEvent> {
        Ok(event)
    }

    #[test]
    fn cancel_after_a_finished_sync_wins() {
        let done = ok(SyncEvent::Nothing);
        assert_eq!(
            next_step(Some(&done), Some(Choice::Cancel), false),
            Step::Cancel
        );
        assert_eq!(next_step(Some(&done), None, false), Step::Go);
    }

    #[test]
    fn a_decision_does_not_wait_for_the_sync() {
        assert_eq!(next_step(None, Some(Choice::Go), false), Step::Go);
        assert_eq!(next_step(None, Some(Choice::Cancel), false), Step::Cancel);
        assert_eq!(next_step(None, None, false), Step::Wait);
    }

    #[test]
    fn a_game_opened_some_other_way_ends_the_flow() {
        let done = ok(SyncEvent::Nothing);
        assert_eq!(next_step(Some(&done), None, true), Step::AlreadyOpen);
        assert_eq!(next_step(None, Some(Choice::Go), true), Step::AlreadyOpen);
    }

    #[test]
    fn waiting_retries_and_errors_mark_stuck() {
        let waiting = ok(SyncEvent::Waiting);
        let failed: CloudResult<SyncEvent> = Err(CloudError::MissingRoot);
        assert_eq!(
            next_step(Some(&waiting), None, false),
            Step::Retry { stuck: false }
        );
        assert_eq!(
            next_step(Some(&failed), None, false),
            Step::Retry { stuck: true }
        );
    }

    #[test]
    fn a_missing_cloud_folder_waits_and_is_stuck_at_once() {
        assert_eq!(
            sync_gate(false, true, Some((true, true))),
            Gate::Wait { stuck: true }
        );
    }

    #[test]
    fn sync_waits_until_the_inputs_are_ready() {
        assert_eq!(sync_gate(true, false, None), Gate::Wait { stuck: false });
        assert_eq!(
            sync_gate(true, false, Some((true, false))),
            Gate::Wait { stuck: false }
        );
    }

    #[test]
    fn ready_inputs_decide_between_sync_and_skip() {
        assert_eq!(sync_gate(true, true, Some((true, true))), Gate::SyncTarget);
        assert_eq!(sync_gate(true, true, Some((false, true))), Gate::Skip);
        assert_eq!(sync_gate(true, true, Some((true, false))), Gate::Skip);
        assert_eq!(sync_gate(true, true, None), Gate::Skip);
    }

    #[test]
    fn admin_only_games_are_retried_through_the_shell() {
        assert!(needs_elevation(&std::io::Error::from_raw_os_error(740)));
        assert!(!needs_elevation(&std::io::Error::from_raw_os_error(2)));
    }

    #[test]
    fn a_late_open_elsewhere_is_shown_and_can_be_replaced() {
        let mut plays = HashMap::new();
        assert!(begin_flow(&mut plays, 7));
        plays.get_mut(&7).unwrap().stage = FlowStage::Elsewhere {
            device: "Notebook".into(),
            since: 10,
        };
        assert_eq!(
            derive_state(plays.get(&7), false, false, true),
            PlayState::OpenElsewhere {
                device: "Notebook".into(),
                since: 10
            }
        );
        assert!(begin_flow(&mut plays, 7));
    }

    fn path(name: &str) -> Option<PathBuf> {
        Some(PathBuf::from(name))
    }

    #[test]
    fn a_chosen_exe_wins_even_over_steam() {
        assert_eq!(
            pick_launch(true, path("ersc_launcher.exe"), None, None),
            LaunchPick::Exe(PathBuf::from("ersc_launcher.exe"))
        );
        assert_eq!(
            pick_launch(
                false,
                path("ersc_launcher.exe"),
                path("eldenring.exe"),
                None
            ),
            LaunchPick::Exe(PathBuf::from("ersc_launcher.exe"))
        );
    }

    #[test]
    fn steam_originals_open_through_steam_by_default() {
        assert_eq!(
            pick_launch(true, None, path("eldenring.exe"), path("eldenring.exe")),
            LaunchPick::Steam
        );
    }

    #[test]
    fn cracked_games_use_the_remembered_then_the_guessed_exe() {
        assert_eq!(
            pick_launch(false, None, path("seen.exe"), path("guess.exe")),
            LaunchPick::Exe(PathBuf::from("seen.exe"))
        );
        assert_eq!(
            pick_launch(false, None, None, path("guess.exe")),
            LaunchPick::Guessed(PathBuf::from("guess.exe"))
        );
        assert_eq!(pick_launch(false, None, None, None), LaunchPick::NeedsExe);
    }
}
