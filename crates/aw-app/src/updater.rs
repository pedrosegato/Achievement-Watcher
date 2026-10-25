use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use aw_core::watcher::unix_now;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::app::AppState;

pub const UPDATE_AVAILABLE: &str = "update-available";
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current: String,
    pub available: Option<String>,
    pub notes: Option<String>,
    pub busy: bool,
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
    let state = app.state::<AppState>();
    {
        let mut status = state.updater.lock();
        if status.busy {
            return;
        }
        status.busy = true;
        status.error = None;
    }
    let result = run_check(app, allow_auto_install);
    let mut status = state.updater.lock();
    status.busy = false;
    status.checked_at = Some(unix_now());
    if let Err(error) = result {
        tracing::info!("update check: {error}");
        status.error = Some(error);
    }
}

fn run_check(app: &AppHandle, allow_auto_install: bool) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let found = tauri::async_runtime::block_on(updater.check()).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let Some(update) = found else {
        let mut status = state.updater.lock();
        status.available = None;
        status.notes = None;
        return Ok(());
    };
    {
        let mut status = state.updater.lock();
        status.available = Some(update.version.clone());
        status.notes = update.body.clone();
    }
    let _ = app.emit(UPDATE_AVAILABLE, update.version.clone());
    let auto = state.settings_snapshot().auto_update;
    let idle = state.running().is_empty();
    if allow_auto_install && auto && idle {
        install(app, update)?;
    }
    Ok(())
}

fn install(app: &AppHandle, update: tauri_plugin_updater::Update) -> Result<(), String> {
    tracing::info!("installing update {}", update.version);
    tauri::async_runtime::block_on(update.download_and_install(|_, _| {}, || {}))
        .map_err(|e| e.to_string())?;
    app.restart();
}

pub fn install_now(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("aw-update-install".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            let result = app
                .updater()
                .map_err(|e| e.to_string())
                .and_then(|u| tauri::async_runtime::block_on(u.check()).map_err(|e| e.to_string()))
                .and_then(|found| found.ok_or_else(|| "no update available".to_owned()))
                .and_then(|update| install(&app, update));
            if let Err(error) = result {
                state.updater.lock().error = Some(error);
            }
        });
    if let Err(error) = spawned {
        tracing::warn!("update install thread failed: {error}");
    }
}
