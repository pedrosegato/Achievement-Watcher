use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use aw_core::cloud::device::DeviceRecord;
use aw_core::cloud::heads::{HeadReason, principal, read_heads};
use aw_core::cloud::key::CloudKey;
use aw_core::cloud::layout::write_json_atomic;
use aw_core::cloud::locate::{Category, Located, SaveRoot, expand};
use aw_core::cloud::lock::foreign_locks;
use aw_core::cloud::portable::{Placeholders, PortablePath};
use aw_core::cloud::snapshot::list_snapshots;
use aw_core::cloud::status::{CloudFileState, folder_size, folder_state};
use aw_core::cloud::steamcloud::DefaultSync;
use aw_core::cloud::sync::last_choice;
use aw_core::watcher::unix_now;
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::app::AppState;
use crate::cloud::{CloudJob, cloud_layout, game_target, inventory, set_game_pref};

const APP_FOLDER: &str = "AchievementWatcher";
const MAX_DEVICE_NAME: usize = 40;
const OVERSIZE_BYTES: u64 = 500 * 1024 * 1024;
const ENV_FOLDERS: [&str; 3] = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"];
const PROFILE_FOLDERS: [&str; 3] = ["Dropbox", "My Drive", "Meu Drive"];
const DRIVE_FOLDERS: [&str; 2] = ["G:\\My Drive", "G:\\Meu Drive"];
const UNDOABLE: [HeadReason; 4] = [
    HeadReason::Restore,
    HeadReason::Branch,
    HeadReason::Manual,
    HeadReason::Undo,
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudGameSize {
    pub app_id: Option<u32>,
    pub key: String,
    pub name: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudOverview {
    pub folder: Option<String>,
    pub folder_missing: bool,
    pub device_name: String,
    pub suggestions: Vec<String>,
    pub total_bytes: u64,
    pub games: Vec<CloudGameSize>,
    pub manifest_ready: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastAction {
    pub reason: HeadReason,
    pub at: i64,
    pub device_name: String,
    pub can_undo: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotView {
    pub id: String,
    pub parent: Option<String>,
    pub created_at: i64,
    pub device_name: String,
    pub playtime_seconds: Option<i64>,
    pub unlocked: Option<u32>,
    pub is_principal: bool,
    pub found_on_device: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudGameView {
    pub key: Option<String>,
    pub enabled: bool,
    pub default: DefaultSync,
    pub located: String,
    pub paths: Vec<String>,
    pub oversize: bool,
    pub state: Option<CloudFileState>,
    pub last_snapshot_at: Option<i64>,
    pub open_elsewhere: Option<String>,
    pub last_action: Option<LastAction>,
    pub snapshots: Vec<SnapshotView>,
}

pub fn suggestions(env: impl Fn(&str) -> Option<String>, fixed: &[PathBuf]) -> Vec<String> {
    let mut roots: Vec<PathBuf> = ENV_FOLDERS
        .iter()
        .filter_map(|name| env(name))
        .map(PathBuf::from)
        .collect();
    if let Some(profile) = env("USERPROFILE") {
        roots.extend(
            PROFILE_FOLDERS
                .iter()
                .map(|name| Path::new(&profile).join(name)),
        );
    }
    roots.extend(fixed.iter().cloned());
    let mut out: Vec<String> = Vec::new();
    for root in roots.into_iter().filter(|root| root.is_dir()) {
        let text = root.join(APP_FOLDER).to_string_lossy().into_owned();
        if !out.contains(&text) {
            out.push(text);
        }
    }
    out
}

pub fn display_paths(roots: &[SaveRoot], ph: &Placeholders) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for root in roots.iter().filter(|root| root.category == Category::Save) {
        let found = expand(root, ph);
        let shown: Vec<String> = if found.is_empty() {
            vec![root.pattern.to_absolute(ph).map_or_else(
                || root.pattern.as_str().to_owned(),
                |p| p.display().to_string(),
            )]
        } else {
            found.iter().map(|p| p.display().to_string()).collect()
        };
        for path in shown {
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

pub fn is_oversize(bytes: u64) -> bool {
    bytes > OVERSIZE_BYTES
}

pub fn clean_device_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && name.chars().count() <= MAX_DEVICE_NAME).then(|| name.to_owned())
}

#[tauri::command]
pub async fn cloud_overview(state: State<'_, AppState>) -> Result<CloudOverview, String> {
    let settings = state.settings_snapshot();
    let layout = cloud_layout(&settings);
    let device_name = state
        .device
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .name
        .clone();
    let fixed: Vec<PathBuf> = DRIVE_FOLDERS.iter().map(PathBuf::from).collect();
    let mut games = Vec::new();
    let mut total_bytes = 0;
    if let Some(layout) = layout.as_ref().filter(|l| l.exists()) {
        total_bytes = folder_size(layout.root());
        for (key, dirs) in layout.games() {
            let name = dirs
                .first()
                .and_then(|dir| dir.0.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .and_then(|n| n.split_once(" - ").map(|(_, name)| name.to_owned()))
                .unwrap_or_else(|| key.as_folder_key());
            let app_id = match &key {
                CloudKey::Steam(id) => Some(*id),
                CloudKey::External(ext) => state.store().external_app_id(ext).ok().flatten(),
            };
            games.push(CloudGameSize {
                app_id,
                key: key.as_folder_key(),
                name,
                bytes: dirs.iter().map(|dir| folder_size(&dir.0)).sum(),
            });
        }
        games.sort_by_key(|game| std::cmp::Reverse(game.bytes));
    }
    Ok(CloudOverview {
        folder: settings
            .cloud_folder
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
        folder_missing: layout.as_ref().is_some_and(|l| !l.exists()),
        device_name,
        suggestions: suggestions(|name| std::env::var(name).ok(), &fixed),
        total_bytes,
        games,
        manifest_ready: state.cloud.manifest().is_some(),
        last_error: state.cloud.last_error(),
    })
}

#[tauri::command]
pub fn set_cloud_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<(), String> {
    let folder = match path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        None => None,
        Some(text) => {
            let path = PathBuf::from(text);
            if !path.is_dir() {
                let parent_exists = path.parent().is_some_and(Path::is_dir);
                if !parent_exists {
                    return Err("folderMissing".into());
                }
                std::fs::create_dir(&path).map_err(|e| e.to_string())?;
            }
            Some(path)
        }
    };
    let mut settings = state.settings_snapshot();
    settings.cloud_folder = folder;
    state.apply_settings(&app, settings)
}

#[tauri::command]
pub fn set_device_name(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let name = clean_device_name(&name).ok_or_else(|| "invalidName".to_owned())?;
    let device = {
        let mut device = state.device.lock().unwrap_or_else(PoisonError::into_inner);
        device.name = name;
        device
            .save(&state.paths.device_file)
            .map_err(|e| e.to_string())?;
        device.clone()
    };
    if let Some(layout) = cloud_layout(&state.settings_snapshot()).filter(|l| l.exists()) {
        write_json_atomic(
            &layout.device_file(&device.id),
            &DeviceRecord {
                name: device.name,
                last_seen: unix_now(),
            },
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn cloud_game(state: State<'_, AppState>, app_id: u32) -> Result<CloudGameView, String> {
    let inv = inventory(&state);
    let Ok(info) = game_target(&state, &inv, app_id) else {
        return Ok(CloudGameView {
            key: None,
            enabled: false,
            default: DefaultSync::Unknown,
            located: "notFound".into(),
            paths: Vec::new(),
            oversize: false,
            state: None,
            last_snapshot_at: None,
            open_elsewhere: None,
            last_action: None,
            snapshots: Vec::new(),
        });
    };
    let target = &info.target;
    let located = match &info.located {
        Located::Roots(_) => "found",
        Located::RegistryOnly => "registry",
        Located::Unsupported => "unsupported",
        Located::NotFound if state.cloud.manifest().is_none() => "waitingManifest",
        Located::NotFound => "notFound",
    };
    let paths = display_paths(&target.roots, &target.placeholders);
    let save_bytes: u64 = target
        .roots
        .iter()
        .flat_map(|root| expand(root, &target.placeholders))
        .map(|path| {
            if path.is_file() {
                std::fs::metadata(&path).map_or(0, |m| m.len())
            } else {
                folder_size(&path)
            }
        })
        .sum();
    let mut view = CloudGameView {
        key: Some(target.key.as_folder_key()),
        enabled: info.enabled,
        default: info.default,
        located: located.into(),
        paths,
        oversize: is_oversize(save_bytes),
        state: None,
        last_snapshot_at: None,
        open_elsewhere: None,
        last_action: None,
        snapshots: Vec::new(),
    };
    let Some(layout) = cloud_layout(&state.settings_snapshot()).filter(|l| l.exists()) else {
        return Ok(view);
    };
    let own = state
        .device
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .id
        .clone();
    let games = layout.game_dirs(&target.key);
    let snapshots = list_snapshots(&games);
    let metas: Vec<_> = snapshots.iter().map(|(_, m)| m.clone()).collect();
    let current = principal(&read_heads(&games), &metas).map(|p| p.snapshot_id);
    if let Some((dir, meta)) = snapshots
        .iter()
        .find(|(_, m)| Some(&m.id) == current.as_ref())
    {
        view.state = Some(folder_state(dir));
        view.last_snapshot_at = Some(meta.created_at);
    }
    view.open_elsewhere = foreign_locks(&games, &own, unix_now())
        .first()
        .map(|lock| lock.device_name.clone());
    view.last_action = last_choice(&layout, &target.key).map(|choice| LastAction {
        can_undo: choice.device_id == own.0
            && choice.previous.is_some()
            && UNDOABLE.contains(&choice.reason),
        reason: choice.reason,
        at: choice.chosen_at,
        device_name: choice.device_name,
    });
    view.snapshots = metas
        .into_iter()
        .map(|meta| SnapshotView {
            is_principal: Some(&meta.id) == current.as_ref(),
            id: meta.id,
            parent: meta.parent,
            created_at: meta.created_at,
            device_name: meta.device_name,
            playtime_seconds: meta.playtime_seconds,
            unlocked: meta.unlocked,
            found_on_device: meta.found_on_device,
        })
        .collect();
    Ok(view)
}

fn update_game_pref(
    app: &AppHandle,
    state: &AppState,
    app_id: u32,
    change: impl FnOnce(
        &mut crate::settings::GameSyncPref,
        &aw_core::cloud::portable::Placeholders,
    ) -> Result<(), String>,
) -> Result<(), String> {
    let inv = inventory(state);
    let info = game_target(state, &inv, app_id)?;
    let mut settings = state.settings_snapshot();
    let mut result = Ok(());
    set_game_pref(&mut settings, &info.target.key, |pref| {
        result = change(pref, &info.target.placeholders);
    });
    result?;
    state.apply_settings(app, settings)?;
    state.cloud.send(CloudJob::ReconcileAll);
    Ok(())
}

#[tauri::command]
pub fn set_game_sync(
    app: AppHandle,
    state: State<'_, AppState>,
    app_id: u32,
    enabled: bool,
) -> Result<(), String> {
    update_game_pref(&app, &state, app_id, |pref, _| {
        pref.enabled = Some(enabled);
        Ok(())
    })
}

#[tauri::command]
pub fn set_game_save_path(
    app: AppHandle,
    state: State<'_, AppState>,
    app_id: u32,
    path: Option<String>,
) -> Result<(), String> {
    update_game_pref(&app, &state, app_id, |pref, placeholders| {
        pref.save_path = match path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            None => None,
            Some(text) => Some(
                PortablePath::from_absolute(Path::new(text), placeholders)
                    .ok_or_else(|| "pathNotPortable".to_owned())?
                    .as_str()
                    .to_owned(),
            ),
        };
        Ok(())
    })
}

#[tauri::command]
pub fn cloud_undo(state: State<'_, AppState>, app_id: u32) {
    state.cloud.send(CloudJob::Undo(app_id));
}

#[tauri::command]
pub fn cloud_set_principal(state: State<'_, AppState>, app_id: u32, snapshot_id: String) {
    state
        .cloud
        .send(CloudJob::SetPrincipal(app_id, snapshot_id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions_only_list_existing_parents() {
        let dir = tempfile::tempdir().unwrap();
        let onedrive = dir.path().join("OneDrive");
        std::fs::create_dir_all(&onedrive).unwrap();
        let env = |name: &str| match name {
            "OneDrive" => Some(onedrive.to_string_lossy().into_owned()),
            "USERPROFILE" => Some(dir.path().to_string_lossy().into_owned()),
            _ => None,
        };
        let got = suggestions(env, &[]);
        assert_eq!(
            got,
            vec![
                onedrive
                    .join("AchievementWatcher")
                    .to_string_lossy()
                    .into_owned()
            ]
        );
        std::fs::create_dir_all(dir.path().join("Dropbox")).unwrap();
        assert_eq!(suggestions(env, &[]).len(), 2);
    }

    #[test]
    fn device_name_is_trimmed_and_bounded() {
        assert_eq!(clean_device_name("  Desktop  ").as_deref(), Some("Desktop"));
        assert_eq!(clean_device_name("   "), None);
        assert_eq!(clean_device_name(&"x".repeat(41)), None);
        assert_eq!(
            clean_device_name(&"x".repeat(40)).map(|n| n.len()),
            Some(40)
        );
    }

    #[test]
    fn save_paths_show_the_real_account_folder() {
        use aw_core::discovery::folders::KnownFolders;
        let dir = tempfile::tempdir().unwrap();
        let ph = Placeholders {
            folders: Some(KnownFolders::under(dir.path())),
            ..Default::default()
        };
        let roots = [SaveRoot {
            category: Category::Save,
            pattern: PortablePath::new("<winAppData>/EldenRing/<storeUserId>").unwrap(),
            single_file: false,
        }];
        let none = display_paths(&roots, &ph);
        assert_eq!(none.len(), 1);
        assert!(none[0].ends_with("<storeUserId>"));
        let account = dir
            .path()
            .join("RoamingAppData")
            .join("EldenRing")
            .join("76561199048130839");
        std::fs::create_dir_all(&account).unwrap();
        assert_eq!(
            display_paths(&roots, &ph),
            vec![account.display().to_string()]
        );
    }

    #[test]
    fn oversize_threshold_allows_large_real_saves() {
        assert!(!is_oversize(116 * 1024 * 1024));
        assert!(is_oversize(600 * 1024 * 1024));
    }
}
