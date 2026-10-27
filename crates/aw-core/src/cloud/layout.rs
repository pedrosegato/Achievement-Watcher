use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::cloud::device::DeviceId;
use crate::cloud::key::CloudKey;
use crate::cloud::utc_stamp;

const GAMES: &str = "games";
const DEVICES: &str = "devices";
const EMULATORS: &str = "emulators";
const README: &str = "README.txt";
const KEY_SEPARATOR: &str = " - ";
const MAX_NAME: usize = 80;
const FALLBACK_NAME: &str = "game";
const RESERVED: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudLayout {
    root: PathBuf,
}

impl CloudLayout {
    pub fn new(root: PathBuf) -> CloudLayout {
        CloudLayout { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn exists(&self) -> bool {
        self.root.is_dir()
    }

    pub fn device_file(&self, id: &DeviceId) -> PathBuf {
        self.root.join(DEVICES).join(format!("{}.json", id.0))
    }

    pub fn settings_dir(&self) -> PathBuf {
        self.root.join("settings")
    }

    pub fn settings_file(&self, id: &DeviceId) -> PathBuf {
        self.settings_dir().join(format!("{}.json", id.0))
    }

    pub fn emulator_dir(&self, emulator: &str) -> PathBuf {
        self.root.join(EMULATORS).join(emulator)
    }

    pub fn game_dirs(&self, key: &CloudKey) -> Vec<GamePaths> {
        self.games()
            .into_iter()
            .find(|(found, _)| found == key)
            .map(|(_, dirs)| dirs)
            .unwrap_or_default()
    }

    pub fn game_dir_for_write(&self, key: &CloudKey, name: &str) -> io::Result<GamePaths> {
        if let Some(existing) = self.game_dirs(key).into_iter().next() {
            return Ok(existing);
        }
        let dir = self.root.join(GAMES).join(format!(
            "{}{KEY_SEPARATOR}{}",
            key.as_folder_key(),
            sanitize_name(name)
        ));
        std::fs::create_dir_all(&dir)?;
        Ok(GamePaths(dir))
    }

    pub fn games(&self) -> Vec<(CloudKey, Vec<GamePaths>)> {
        let Ok(entries) = std::fs::read_dir(self.root.join(GAMES)) else {
            return Vec::new();
        };
        let mut dirs: Vec<(CloudKey, PathBuf)> = entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| {
                let name = entry.file_name().to_str()?.to_owned();
                let prefix = name
                    .split_once(KEY_SEPARATOR)
                    .map_or(name.as_str(), |p| p.0);
                Some((CloudKey::parse(prefix)?, entry.path()))
            })
            .collect();
        dirs.sort();
        let mut grouped: Vec<(CloudKey, Vec<GamePaths>)> = Vec::new();
        for (key, dir) in dirs {
            match grouped.last_mut() {
                Some((last, list)) if *last == key => list.push(GamePaths(dir)),
                _ => grouped.push((key, vec![GamePaths(dir)])),
            }
        }
        grouped
    }

    pub fn changed_games(&self, paths: &[PathBuf]) -> Option<BTreeSet<CloudKey>> {
        let mut keys = BTreeSet::new();
        for path in paths {
            let relative = path.strip_prefix(&self.root).ok()?;
            let mut parts = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned());
            match parts.next().as_deref() {
                Some(GAMES) => {
                    let folder = parts.next()?;
                    let (prefix, _) = folder.split_once(KEY_SEPARATOR)?;
                    keys.insert(CloudKey::parse(prefix)?);
                }
                Some(DEVICES | EMULATORS | README) => {}
                _ => return None,
            }
        }
        Some(keys)
    }

    pub fn write_readme(&self, text: &str) -> io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::write(self.root.join(README), text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamePaths(pub PathBuf);

impl GamePaths {
    pub fn heads_dir(&self) -> PathBuf {
        self.0.join("heads")
    }

    pub fn locks_dir(&self) -> PathBuf {
        self.0.join("locks")
    }

    pub fn snapshots_dir(&self) -> PathBuf {
        self.0.join("snapshots")
    }

    pub fn head_file(&self, id: &DeviceId) -> PathBuf {
        self.heads_dir().join(format!("{}.json", id.0))
    }

    pub fn lock_file(&self, id: &DeviceId) -> PathBuf {
        self.locks_dir().join(format!("{}.json", id.0))
    }
}

pub fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !RESERVED.contains(c) && !c.is_control())
        .collect();
    let trimmed: String = cleaned
        .trim_end_matches(['.', ' '])
        .trim_start()
        .chars()
        .take(MAX_NAME)
        .collect();
    let trimmed = trimmed.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        FALLBACK_NAME.to_owned()
    } else {
        trimmed.to_owned()
    }
}

pub fn snapshot_folder_name(created_at: i64, device_name: &str, id: &str) -> String {
    let device = sanitize_name(device_name).to_lowercase().replace(' ', "-");
    format!("{}_{device}_{id}", utc_stamp(created_at))
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&temp, path)
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn snapshot_folder_name_is_utc_and_sanitized() {
        assert_eq!(
            snapshot_folder_name(1_793_050_200, "Desktop PC", "ab12cd"),
            "2026-10-26_21-30_desktop-pc_ab12cd"
        );
    }

    #[test]
    fn sanitize_name_strips_reserved_characters() {
        assert_eq!(sanitize_name("Dark Souls: III?. "), "Dark Souls III");
        assert_eq!(sanitize_name("***"), "game");
        assert_eq!(sanitize_name(&"a".repeat(200)).len(), 80);
    }

    #[test]
    fn game_dir_for_write_creates_then_reuses() {
        let dir = tempfile::tempdir().unwrap();
        let layout = CloudLayout::new(dir.path().to_path_buf());
        let key = CloudKey::Steam(374320);
        let first = layout.game_dir_for_write(&key, "Dark Souls: III").unwrap();
        assert!(first.0.ends_with("374320 - Dark Souls III"));
        let again = layout.game_dir_for_write(&key, "Renamed").unwrap();
        assert_eq!(first, again);
        assert_eq!(layout.games().len(), 1);
    }

    #[test]
    fn duplicate_game_folders_are_merged() {
        let dir = tempfile::tempdir().unwrap();
        let layout = CloudLayout::new(dir.path().to_path_buf());
        let games = dir.path().join("games");
        std::fs::create_dir_all(games.join("374320 - Dark Souls III")).unwrap();
        std::fs::create_dir_all(games.join("374320 - Dark Souls III (1)")).unwrap();
        std::fs::create_dir_all(games.join("uplay-4 - AC2")).unwrap();
        std::fs::create_dir_all(games.join("not a game")).unwrap();
        assert_eq!(layout.game_dirs(&CloudKey::Steam(374320)).len(), 2);
        assert_eq!(layout.games().len(), 2);
    }

    #[test]
    fn game_paths_are_under_the_game_folder() {
        let game = GamePaths(PathBuf::from("g"));
        let id = DeviceId("abc".into());
        assert_eq!(
            game.head_file(&id),
            PathBuf::from("g").join("heads").join("abc.json")
        );
        assert_eq!(
            game.lock_file(&id),
            PathBuf::from("g").join("locks").join("abc.json")
        );
        assert_eq!(game.snapshots_dir(), PathBuf::from("g").join("snapshots"));
    }

    #[test]
    fn json_round_trips_atomically_and_invalid_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("x.json");
        write_json_atomic(&path, &vec![1, 2]).unwrap();
        assert_eq!(read_json::<Vec<u32>>(&path), Some(vec![1, 2]));
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::write(&path, "{").unwrap();
        assert_eq!(read_json::<Vec<u32>>(&path), None);
        assert_eq!(read_json::<Vec<u32>>(&dir.path().join("none.json")), None);
    }

    #[test]
    fn changes_inside_a_game_folder_touch_only_that_game() {
        let layout = CloudLayout::new(PathBuf::from(r"G:\My Drive\AW"));
        let paths = vec![
            PathBuf::from(r"G:\My Drive\AW\games\374320 - Dark Souls III\heads\desk.json"),
            PathBuf::from(
                r"G:\My Drive\AW\games\374320 - Dark Souls III\snapshots\x\snapshot.json",
            ),
            PathBuf::from(r"G:\My Drive\AW\games\uplay-4 - Far Cry\locks\nb.json"),
            PathBuf::from(r"G:\My Drive\AW\devices\nb.json"),
            PathBuf::from(r"G:\My Drive\AW\README.txt"),
        ];
        let keys = layout.changed_games(&paths).unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&CloudKey::Steam(374_320)));
        assert!(keys.contains(&CloudKey::parse("uplay-4").unwrap()));
    }

    #[test]
    fn settings_or_unknown_paths_touch_every_game() {
        let layout = CloudLayout::new(PathBuf::from(r"G:\My Drive\AW"));
        let settings = vec![PathBuf::from(r"G:\My Drive\AW\settings\nb.json")];
        assert_eq!(layout.changed_games(&settings), None);
        let outside = vec![PathBuf::from(r"D:\elsewhere\file.json")];
        assert_eq!(layout.changed_games(&outside), None);
        let games_root = vec![PathBuf::from(r"G:\My Drive\AW\games")];
        assert_eq!(layout.changed_games(&games_root), None);
        let root = vec![PathBuf::from(r"G:\My Drive\AW")];
        assert_eq!(layout.changed_games(&root), None);
        let short_name = vec![PathBuf::from(r"G:\My Drive\AW\games\UPLAY-~1\heads\a.json")];
        assert_eq!(layout.changed_games(&short_name), None);
    }
}
