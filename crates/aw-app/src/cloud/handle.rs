use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

use aw_core::cloud::capture::HashCache;
use aw_core::cloud::manifest::ManifestIndex;
use aw_core::cloud::sync::{GameTarget, SyncEvent};
use aw_core::cloud::{CloudError, CloudResult};
use tauri::{AppHandle, Emitter, Manager};

use crate::app::{AppState, GAMES_CHANGED};
use crate::cloud_status::{Activity, ActivityEvent, CLOUD_ACTIVITY, Outcome, outcome_changed};

use aw_core::cloud::device::{DeviceId, LocalDevice};
use aw_core::cloud::key::CloudKey;
use aw_core::cloud::layout::CloudLayout;
use aw_core::cloud::lock::{LockRecord, remove_own_lock, write_lock};
use aw_core::watcher::unix_now;

use super::*;

pub struct CloudHandle {
    pub jobs: Mutex<Sender<CloudJob>>,
    pub manifest: RwLock<Option<Arc<ManifestIndex>>>,
    pub manifest_state: Mutex<ManifestState>,
    pub last_error: Mutex<Option<String>>,
    pub game_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    pub last_change: Arc<Mutex<Option<Instant>>>,
    pub activity: Mutex<HashMap<String, Activity>>,
    pub cache: Arc<HashCache>,
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
            activity: Mutex::new(HashMap::new()),
            cache: Arc::new(HashCache::default()),
        }
    }

    pub fn activity_of(&self, folder_key: &str) -> Activity {
        self.activity
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(folder_key)
            .cloned()
            .unwrap_or_default()
    }

    fn set_syncing(&self, folder_key: &str) {
        self.activity
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(folder_key.to_owned())
            .or_default()
            .syncing = true;
    }

    fn clear_syncing(&self, folder_key: &str) {
        if let Some(entry) = self
            .activity
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(folder_key)
        {
            entry.syncing = false;
        }
    }

    fn finish_sync(&self, folder_key: &str, outcome: Option<Outcome>) -> bool {
        let mut map = self.activity.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = map.entry(folder_key.to_owned()).or_default();
        entry.syncing = false;
        let Some(outcome) = outcome else {
            return false;
        };
        let changed = outcome_changed(entry.last.as_ref(), Some(&outcome));
        entry.last = Some(outcome);
        changed
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

    pub(crate) fn set_manifest(&self, index: Option<ManifestIndex>, state: ManifestState) {
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

    pub(crate) fn set_error(&self, error: Option<String>) {
        *self
            .last_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = error;
    }
}

pub fn error_code(error: &CloudError) -> String {
    match error {
        CloudError::MissingRoot => "missingRoot".to_owned(),
        CloudError::MultipleAccounts(_) => "multipleAccounts".to_owned(),
        other => other.to_string(),
    }
}

pub fn tracked(
    app: &AppHandle,
    target: &GameTarget,
    work: impl FnOnce() -> CloudResult<SyncEvent>,
) -> CloudResult<SyncEvent> {
    let key = target.key.as_folder_key();
    let cloud = &app.state::<AppState>().cloud;
    let tell = |syncing| {
        let event = ActivityEvent {
            app_id: target.app_id,
            syncing,
        };
        let _ = app.emit(CLOUD_ACTIVITY, event);
    };
    cloud.set_syncing(&key);
    let guard = SyncingGuard { cloud, key: &key };
    tell(true);
    let result = work();
    drop(guard);
    let outcome = match &result {
        Ok(event) => Some(Outcome::Synced(event.clone())),
        Err(CloudError::Cancelled) => None,
        Err(error) => Some(Outcome::Failed(error_code(error))),
    };
    let changed = cloud.finish_sync(&key, outcome);
    tell(false);
    if changed {
        let _ = app.emit(CLOUD_CHANGED, Some(target.app_id));
    }
    result
}

struct SyncingGuard<'a> {
    cloud: &'a CloudHandle,
    key: &'a str,
}

impl Drop for SyncingGuard<'_> {
    fn drop(&mut self) {
        self.cloud.clear_syncing(self.key);
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

pub fn local_device(state: &AppState) -> LocalDevice {
    state
        .device
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

pub fn mark_open(
    layout: &CloudLayout,
    target: &GameTarget,
    device: &LocalDevice,
) -> std::io::Result<()> {
    let game = layout.game_dir_for_write(&target.key, &target.name)?;
    let now = unix_now();
    write_lock(
        &game,
        &device.id,
        &LockRecord {
            device_id: device.id.0.clone(),
            device_name: device.name.clone(),
            game_key: target.key.as_folder_key(),
            started_at: now,
            renewed_at: now,
        },
    )
}

pub fn release_lock(
    layout: &CloudLayout,
    device: &DeviceId,
    key: &CloudKey,
) -> std::io::Result<()> {
    for game in layout.game_dirs(key) {
        remove_own_lock(&game, device)?;
    }
    Ok(())
}
