use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use aw_core::watcher::unix_now;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::app::AppState;

pub const UPDATE_AVAILABLE: &str = "update-available";
pub const UPDATE_PROGRESS: &str = "update-progress";
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
const PROGRESS_EVERY: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    Downloading,
    Installing,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current: String,
    pub available: Option<String>,
    pub notes: Option<String>,
    pub phase: UpdatePhase,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
    pub checked_at: Option<i64>,
}

#[derive(Default)]
pub struct UpdaterState {
    status: Mutex<UpdateStatus>,
}

impl UpdaterState {
    fn lock(&self) -> MutexGuard<'_, UpdateStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub fn status(state: &AppState) -> UpdateStatus {
    let mut status = state.updater.lock().clone();
    status.current = env!("CARGO_PKG_VERSION").into();
    status
}

fn report(app: &AppHandle) {
    let state = app.state::<AppState>();
    let _ = app.emit(UPDATE_PROGRESS, status(&state));
}

fn change(app: &AppHandle, f: impl FnOnce(&mut UpdateStatus)) {
    f(&mut app.state::<AppState>().updater.lock());
    report(app);
}

fn begin(app: &AppHandle) -> bool {
    {
        let state = app.state::<AppState>();
        let mut status = state.updater.lock();
        if status.phase != UpdatePhase::Idle {
            return false;
        }
        status.phase = UpdatePhase::Checking;
        status.error = None;
    }
    report(app);
    true
}

fn finish(app: &AppHandle, result: Result<(), String>) {
    change(app, |status| {
        status.phase = UpdatePhase::Idle;
        status.checked_at = Some(unix_now());
        if let Err(error) = result {
            tracing::info!("update: {error}");
            status.error = Some(error);
        }
    });
}

pub fn spawn(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("aw-updater".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK_DELAY);
            loop {
                check(&app, true);
                std::thread::sleep(CHECK_EVERY);
            }
        });
    if let Err(error) = spawned {
        tracing::warn!("updater thread failed: {error}");
    }
}

pub fn check(app: &AppHandle, allow_auto_install: bool) {
    if begin(app) {
        finish(app, run_check(app, allow_auto_install));
    }
}

fn find(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let found = tauri::async_runtime::block_on(updater.check()).map_err(|e| e.to_string())?;
    change(app, |status| {
        status.available = found.as_ref().map(|u| u.version.clone());
        status.notes = found.as_ref().and_then(|u| u.body.clone());
    });
    Ok(found)
}

fn run_check(app: &AppHandle, allow_auto_install: bool) -> Result<(), String> {
    let Some(update) = find(app)? else {
        return Ok(());
    };
    let _ = app.emit(UPDATE_AVAILABLE, update.version.clone());
    let state = app.state::<AppState>();
    let auto = state.settings_snapshot().auto_update;
    let idle = state.running().is_empty();
    if allow_auto_install && auto && idle {
        install(app, update)?;
    }
    Ok(())
}

fn install(app: &AppHandle, update: tauri_plugin_updater::Update) -> Result<(), String> {
    tracing::info!("installing update {}", update.version);
    change(app, |status| {
        status.phase = UpdatePhase::Downloading;
        status.downloaded = 0;
        status.total = None;
    });
    let mut last = Instant::now();
    tauri::async_runtime::block_on(update.download_and_install(
        |chunk, total| {
            {
                let state = app.state::<AppState>();
                let mut status = state.updater.lock();
                status.downloaded += chunk as u64;
                status.total = total;
            }
            if last.elapsed() >= PROGRESS_EVERY {
                last = Instant::now();
                report(app);
            }
        },
        || change(app, |status| status.phase = UpdatePhase::Installing),
    ))
    .map_err(|e| e.to_string())?;
    app.restart();
}

pub fn install_now(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("aw-update-install".into())
        .spawn(move || {
            if !begin(&app) {
                return;
            }
            let result = find(&app)
                .and_then(|found| found.ok_or_else(|| "no update available".to_owned()))
                .and_then(|update| install(&app, update));
            finish(&app, result);
        });
    if let Err(error) = spawned {
        tracing::warn!("update install thread failed: {error}");
    }
}
