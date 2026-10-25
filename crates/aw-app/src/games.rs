use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use aw_core::discovery::running::{game_root_for_exe, is_system_exe, list_processes, match_known};
use aw_core::discovery::steam::{
    localconfig_files, localconfig_playtimes, normalize_for_compare, steam_install_path,
};
use aw_core::watcher::unix_now;
use tauri::{AppHandle, Emitter, Manager};

use crate::app::{AppState, GAMES_CHANGED};
use crate::video;

const POLL: Duration = Duration::from_secs(3);
const FLUSH_SECS: i64 = 60;
const STEAM_PLAYTIME_STAMP: &str = "steam_playtime_mtimes";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunningGame {
    pub since: i64,
    pub flushed_at: i64,
}

pub type RunningGames = BTreeMap<u32, RunningGame>;

pub fn run_process_monitor(app: AppHandle) {
    let mut by_pid: HashMap<u32, u32> = HashMap::new();
    let mut ignored: HashSet<u32> = HashSet::new();
    loop {
        std::thread::sleep(POLL);
        let processes = list_processes();
        let alive: HashSet<u32> = processes.iter().map(|p| p.pid).collect();
        let state = app.state::<AppState>();
        let now = unix_now();
        let mut changed = false;

        let gone: Vec<(u32, u32)> = by_pid
            .iter()
            .filter(|(pid, _)| !alive.contains(pid))
            .map(|(pid, app_id)| (*pid, *app_id))
            .collect();
        for (pid, app_id) in gone {
            by_pid.remove(&pid);
            if !by_pid.values().any(|id| *id == app_id) {
                stop(&state, app_id, now);
                video::stop(&app, Some(app_id));
                changed = true;
            }
        }
        ignored.retain(|pid| alive.contains(pid));

        let known = state
            .with_watcher(|w| w.snapshot().known_games)
            .unwrap_or_default();
        for process in processes {
            if by_pid.contains_key(&process.pid) || ignored.contains(&process.pid) {
                continue;
            }
            if is_system_exe(&process.exe) {
                ignored.insert(process.pid);
                continue;
            }
            let app_id = match_known(&process.exe, &known).or_else(|| {
                let (root, app_id) = game_root_for_exe(&process.exe)?;
                let app_id = app_id?;
                remember_game_dir(&app, &state, root);
                Some(app_id)
            });
            match app_id {
                Some(app_id) => {
                    by_pid.insert(process.pid, app_id);
                    if start(&state, app_id, now, &process.exe) {
                        video::start(&app, app_id);
                        changed = true;
                    }
                }
                None => {
                    ignored.insert(process.pid);
                }
            }
        }

        flush(&state, now);
        if changed {
            state.with_watcher(|w| w.request_full_sync());
            let _ = app.emit(GAMES_CHANGED, ());
        }
    }
}

fn start(state: &AppState, app_id: u32, now: i64, exe: &std::path::Path) -> bool {
    let mut running = state.running();
    if running.contains_key(&app_id) {
        return false;
    }
    tracing::info!("game {app_id} started: {}", exe.display());
    running.insert(
        app_id,
        RunningGame {
            since: now,
            flushed_at: now,
        },
    );
    true
}

fn stop(state: &AppState, app_id: u32, now: i64) {
    let Some(game) = state.running().remove(&app_id) else {
        return;
    };
    tracing::info!("game {app_id} stopped after {} s", now - game.since);
    record(state, app_id, now - game.flushed_at, now);
}

fn record(state: &AppState, app_id: u32, seconds: i64, now: i64) {
    let store = state.store();
    if store.steam_minutes(app_id).ok().flatten().is_some() {
        return;
    }
    if let Err(error) = store.add_playtime(app_id, seconds, now) {
        tracing::warn!("cannot save playtime for {app_id}: {error}");
    }
}

pub fn snapshot_seconds(
    steam_minutes: Option<u64>,
    aw_seconds: i64,
    session_elapsed: Option<i64>,
) -> Option<i64> {
    let base = steam_minutes.map_or(aw_seconds, |m| {
        i64::try_from(m.saturating_mul(60)).unwrap_or(i64::MAX)
    });
    let total = base.saturating_add(session_elapsed.unwrap_or(0).max(0));
    (total > 0).then_some(total)
}

pub fn playtime_now(state: &AppState, app_id: u32, now: i64) -> Option<i64> {
    let (steam, aw) = {
        let store = state.store();
        (
            store.steam_minutes(app_id).ok().flatten(),
            store.aw_playtime_seconds(app_id).unwrap_or(0),
        )
    };
    let session = state.running().get(&app_id).map(|game| {
        if steam.is_some() {
            now - game.since
        } else {
            now - game.flushed_at
        }
    });
    snapshot_seconds(steam, aw, session)
}

pub fn refresh_steam_playtime(state: &AppState) {
    let Some(steam) = steam_install_path() else {
        return;
    };
    let stamp: String = localconfig_files(&steam)
        .iter()
        .map(|path| {
            let modified = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            format!("{}={modified}", path.display())
        })
        .collect::<Vec<_>>()
        .join(";");
    let store = state.store();
    if store
        .meta_get(STEAM_PLAYTIME_STAMP)
        .ok()
        .flatten()
        .as_deref()
        == Some(stamp.as_str())
    {
        return;
    }
    let playtimes = localconfig_playtimes(&steam);
    if let Err(error) = store
        .replace_steam_playtime(&playtimes)
        .and_then(|()| store.meta_set(STEAM_PLAYTIME_STAMP, &stamp))
    {
        tracing::warn!("cannot save steam playtime: {error}");
    }
}

fn flush(state: &AppState, now: i64) {
    let due: Vec<(u32, i64)> = state
        .running()
        .iter_mut()
        .filter(|(_, game)| now - game.flushed_at >= FLUSH_SECS)
        .map(|(app_id, game)| {
            let seconds = now - game.flushed_at;
            game.flushed_at = now;
            (*app_id, seconds)
        })
        .collect();
    for (app_id, seconds) in due {
        record(state, app_id, seconds, now);
    }
}

fn remember_game_dir(app: &AppHandle, state: &AppState, root: PathBuf) {
    let mut settings = state.settings_snapshot();
    let key = normalize_for_compare(&root);
    let covered = settings
        .extra_game_dirs
        .iter()
        .chain(settings.extra_game_roots.iter())
        .any(|dir| {
            let dir = normalize_for_compare(dir);
            key == dir || key.starts_with(&format!("{dir}\\"))
        });
    if covered {
        return;
    }
    tracing::info!("adding game folder {}", root.display());
    settings.extra_game_dirs.push(root);
    if let Err(error) = state.apply_settings(app, settings) {
        tracing::warn!("cannot save game folder: {error}");
    }
}

pub fn flush_all(state: &AppState) {
    let now = unix_now();
    let games: Vec<(u32, RunningGame)> = state.running().iter().map(|(k, v)| (*k, *v)).collect();
    for (app_id, game) in games {
        record(state, app_id, now - game.flushed_at, now);
    }
    state.running().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_prefers_steam_plus_session() {
        assert_eq!(snapshot_seconds(Some(470), 999, Some(60)), Some(28_260));
        assert_eq!(snapshot_seconds(None, 3600, Some(120)), Some(3720));
        assert_eq!(snapshot_seconds(None, 0, None), None);
    }
}
