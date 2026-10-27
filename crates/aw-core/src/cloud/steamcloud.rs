use std::io;
use std::path::Path;

use serde::Serialize;

use crate::cloud::key::GameKind;
use crate::cloud::locate::{SaveRoot, expand};
use crate::cloud::portable::Placeholders;
use crate::cloud::{CloudError, CloudResult};

pub const STEAM_CLOUD_CATEGORY: u64 = 23;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultSync {
    On,
    SteamCloud,
    Unknown,
}

pub fn parse_has_cloud(json: &str, app_id: u32) -> Option<bool> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let entry = value.get(app_id.to_string())?;
    if entry.get("success")?.as_bool() != Some(true) {
        return None;
    }
    let categories = entry
        .get("data")
        .and_then(|data| data.get("categories"))
        .and_then(serde_json::Value::as_array);
    Some(categories.is_some_and(|list| {
        list.iter()
            .any(|c| c.get("id").and_then(serde_json::Value::as_u64) == Some(STEAM_CLOUD_CATEGORY))
    }))
}

pub fn fetch_has_cloud(agent: &ureq::Agent, app_id: u32) -> CloudResult<Option<bool>> {
    let url =
        format!("https://store.steampowered.com/api/appdetails?appids={app_id}&filters=categories");
    let text = agent
        .get(&url)
        .call()
        .and_then(|mut response| response.body_mut().read_to_string())
        .map_err(|error| CloudError::Io(io::Error::other(error.to_string())))?;
    Ok(parse_has_cloud(&text, app_id))
}

pub fn default_sync(
    kind: GameKind,
    steam_cloud: Option<bool>,
    manifest_cloud: bool,
) -> DefaultSync {
    match (kind, steam_cloud) {
        (GameKind::Emulator, _) => DefaultSync::On,
        (GameKind::SteamOriginal, Some(true)) => DefaultSync::SteamCloud,
        (GameKind::SteamOriginal, _) if manifest_cloud => DefaultSync::SteamCloud,
        (GameKind::SteamOriginal, Some(false)) => DefaultSync::On,
        (GameKind::SteamOriginal, None) => DefaultSync::Unknown,
    }
}

const AUTOCLOUD_MARKER: &str = "steam_autocloud.vdf";
const REMOTE_CACHE: &str = "remotecache.vdf";
const MARKER_DEPTH: usize = 4;

pub fn has_autocloud_marker(roots: &[SaveRoot], ph: &Placeholders) -> bool {
    roots
        .iter()
        .flat_map(|root| expand(root, ph))
        .any(|path| contains_marker(&path, MARKER_DEPTH))
}

fn contains_marker(path: &Path, depth: usize) -> bool {
    let is_marker = |p: &Path| {
        p.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .eq_ignore_ascii_case(AUTOCLOUD_MARKER)
        })
    };
    if path.is_file() {
        return is_marker(path);
    }
    depth > 0
        && std::fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| contains_marker(&entry.path(), depth - 1))
}

pub fn has_steam_remotecache(app_id: u32, ph: &Placeholders) -> bool {
    let Some(steam) = ph.steam_root.as_ref() else {
        return false;
    };
    std::fs::read_dir(steam.join("userdata"))
        .into_iter()
        .flatten()
        .flatten()
        .any(|account| {
            account
                .path()
                .join(app_id.to_string())
                .join(REMOTE_CACHE)
                .is_file()
        })
}

pub fn sync_enabled(default: DefaultSync, user: Option<bool>) -> bool {
    user.unwrap_or(default == DefaultSync::On)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::locate::Category;
    use crate::cloud::portable::PortablePath;
    use crate::discovery::folders::KnownFolders;

    const ELDEN: &str = r#"{"1245620":{"success":true,"data":{"categories":[{"id":2,"description":"Single-player"},{"id":23,"description":"Steam Cloud"}]}}}"#;
    const DS3: &str = r#"{"374320":{"success":true,"data":{"categories":[{"id":2,"description":"Single-player"},{"id":22,"description":"Steam Achievements"}]}}}"#;
    const NO_CATEGORIES: &str = r#"{"10":{"success":true,"data":{}}}"#;
    const FAILED: &str = r#"{"999":{"success":false}}"#;

    #[test]
    fn elden_ring_category_23_means_cloud() {
        assert_eq!(parse_has_cloud(ELDEN, 1245620), Some(true));
    }

    #[test]
    fn ds3_without_category_means_no_cloud() {
        assert_eq!(parse_has_cloud(DS3, 374320), Some(false));
        assert_eq!(parse_has_cloud(NO_CATEGORIES, 10), Some(false));
    }

    #[test]
    fn failed_lookup_is_unknown() {
        assert_eq!(parse_has_cloud(FAILED, 999), None);
        assert_eq!(parse_has_cloud("not json", 999), None);
        assert_eq!(parse_has_cloud(DS3, 1), None);
    }

    #[test]
    fn emulator_games_default_on_even_if_steam_has_cloud() {
        assert_eq!(
            default_sync(GameKind::Emulator, Some(true), true),
            DefaultSync::On
        );
    }

    #[test]
    fn steam_original_follows_the_store() {
        assert_eq!(
            default_sync(GameKind::SteamOriginal, Some(true), false),
            DefaultSync::SteamCloud
        );
        assert_eq!(
            default_sync(GameKind::SteamOriginal, Some(false), false),
            DefaultSync::On
        );
    }

    #[test]
    fn manifest_cloud_wins_over_a_store_without_the_category() {
        assert_eq!(
            default_sync(GameKind::SteamOriginal, Some(false), true),
            DefaultSync::SteamCloud
        );
    }

    #[test]
    fn autocloud_marker_means_steam_cloud() {
        let dir = tempfile::tempdir().unwrap();
        let ph = Placeholders {
            folders: Some(KnownFolders::under(dir.path())),
            ..Default::default()
        };
        let save = dir
            .path()
            .join("Documents")
            .join("NBGI")
            .join("DSR")
            .join("1087865111");
        std::fs::create_dir_all(&save).unwrap();
        std::fs::write(save.join("DRAKS0005.sl2"), "x").unwrap();
        let roots = [SaveRoot {
            category: Category::Save,
            pattern: PortablePath::new("<winDocuments>/NBGI/DSR").unwrap(),
            single_file: false,
        }];
        assert!(!has_autocloud_marker(&roots, &ph));
        std::fs::write(save.join("steam_autocloud.vdf"), "\"AutoCloud\"").unwrap();
        assert!(has_autocloud_marker(&roots, &ph));
    }

    #[test]
    fn manifest_hint_disables_when_offline() {
        assert_eq!(
            default_sync(GameKind::SteamOriginal, None, true),
            DefaultSync::SteamCloud
        );
        assert_eq!(
            default_sync(GameKind::SteamOriginal, None, false),
            DefaultSync::Unknown
        );
    }

    #[test]
    fn user_choice_wins() {
        assert!(sync_enabled(DefaultSync::SteamCloud, Some(true)));
        assert!(!sync_enabled(DefaultSync::On, Some(false)));
        assert!(sync_enabled(DefaultSync::On, None));
        assert!(!sync_enabled(DefaultSync::Unknown, None));
        assert!(!sync_enabled(DefaultSync::SteamCloud, None));
    }

    #[test]
    fn steam_remotecache_means_steam_cloud() {
        let dir = tempfile::tempdir().unwrap();
        let ph = Placeholders {
            steam_root: Some(dir.path().join("Steam")),
            ..Default::default()
        };
        assert!(!has_steam_remotecache(570940, &ph));
        let game = dir
            .path()
            .join("Steam")
            .join("userdata")
            .join("1087865111")
            .join("570940");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("remotecache.vdf"), "x").unwrap();
        assert!(has_steam_remotecache(570940, &ph));
        assert!(!has_steam_remotecache(374320, &ph));
    }
}
