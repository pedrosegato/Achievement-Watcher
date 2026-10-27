use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use aw_core::cloud::heads::{principal, read_heads};
use aw_core::cloud::key::{CloudKey, GameKind, cloud_key_for, game_kind};
use aw_core::cloud::layout::CloudLayout;
use aw_core::cloud::locate::{LocateInput, Located, SaveRoot, locate};
use aw_core::cloud::manifest::ManifestIndex;
use aw_core::cloud::portable::{Placeholders, PortablePath};
use aw_core::cloud::snapshot::list_snapshots;
use aw_core::cloud::steamcloud::{
    DefaultSync, default_sync, has_autocloud_marker, has_steam_remotecache, sync_enabled,
};
use aw_core::cloud::sync::GameTarget;
use aw_core::discovery::Found;
use aw_core::discovery::running::KnownGame;
use aw_core::discovery::steam::{installed_apps, library_folders, steam_install_path};

use crate::app::AppState;
use crate::settings::{GameSyncPref, Settings};

use super::*;

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
    pub(crate) found: Vec<Found>,
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

pub(crate) fn cloud_only_target(
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

pub fn set_game_pref(
    settings: &mut Settings,
    key: &CloudKey,
    change: impl FnOnce(&mut GameSyncPref),
) {
    let pref = settings.game_sync.entry(key.as_folder_key()).or_default();
    change(pref);
}
