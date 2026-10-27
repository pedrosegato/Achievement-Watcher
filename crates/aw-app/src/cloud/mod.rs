use std::path::PathBuf;

use aw_core::cloud::snapshot::Contents;

use crate::settings::Settings;

mod handle;
mod target;
mod worker;

pub use handle::*;
pub use target::*;
pub use worker::*;

pub const CLOUD_CHANGED: &str = "cloud-changed";

pub enum CloudJob {
    Startup,
    GameStarted(u32),
    GameExited(u32),
    ReconcileAll,
    SettingsChanged {
        old: Box<Settings>,
        new: Box<Settings>,
    },
    CloudFilesChanged(Vec<PathBuf>),
    Undo(u32),
    SetPrincipal(u32, String),
    SyncGame(u32),
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};
    use std::path::Path;

    use aw_core::cloud::key::CloudKey;
    use aw_core::cloud::sync::SyncEvent;

    use crate::settings::GameSyncPref;

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
