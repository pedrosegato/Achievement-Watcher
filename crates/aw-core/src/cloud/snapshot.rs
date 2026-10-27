use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cloud::capture::{Capture, category_name, file_sha256};
use crate::cloud::device::LocalDevice;
use crate::cloud::layout::{GamePaths, read_json, snapshot_folder_name, write_json_atomic};
use crate::cloud::locate::Category;
use crate::cloud::portable::PortablePath;
use crate::cloud::{CloudError, CloudResult, at, copy_file, random_id};
use crate::store::GameState;

pub const FORMAT_VERSION: u32 = 1;
pub const SNAPSHOT_FILE: &str = "snapshot.json";
const STATE_FILE: &str = "state.json";
const TMP_PREFIX: &str = ".tmp-";
const ID_LEN: usize = 12;
const SHORT_ID_LEN: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contents {
    pub save: bool,
    pub emulator_files: bool,
    pub state: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub category: Category,
    pub stored: String,
    pub original: PortablePath,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub format: u32,
    pub id: String,
    pub game_key: String,
    pub game_name: String,
    pub device_id: String,
    pub device_name: String,
    pub created_at: i64,
    pub session_ended_at: i64,
    pub parent: Option<String>,
    pub found_on_device: bool,
    /// A copy of this PC's state saved just before the cloud overwrote it,
    /// not progress someone chose to keep playing from.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kept: bool,
    pub contents: Contents,
    pub files: Vec<FileEntry>,
    pub digest: String,
    pub playtime_seconds: Option<i64>,
    pub unlocked: Option<u32>,
    pub local_app_id: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_file: Option<StoredState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredState {
    pub sha256: String,
    pub size: u64,
}

pub struct NewSnapshot<'a> {
    pub game: &'a GamePaths,
    pub game_key: &'a str,
    pub game_name: &'a str,
    pub device: &'a LocalDevice,
    pub now: i64,
    pub session_ended_at: i64,
    pub parent: Option<String>,
    pub found_on_device: bool,
    pub kept: bool,
    pub local_app_id: u32,
    pub capture: &'a Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verify {
    Sizes,
    Hashes,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SnapshotRead {
    Ready(Box<SnapshotMeta>),
    Incomplete,
}

pub fn write_snapshot(new: &NewSnapshot) -> CloudResult<SnapshotMeta> {
    let id = random_id(ID_LEN);
    let snapshots = new.game.snapshots_dir();
    let tmp = snapshots.join(format!("{TMP_PREFIX}{id}"));
    let result = build(new, &id, &tmp).and_then(|meta| {
        let name = snapshot_folder_name(new.now, &new.device.name, &id[..SHORT_ID_LEN]);
        let target = snapshots.join(name);
        std::fs::rename(&tmp, &target).map_err(at(&target))?;
        Ok(meta)
    });
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    result
}

fn build(new: &NewSnapshot, id: &str, tmp: &Path) -> CloudResult<SnapshotMeta> {
    std::fs::create_dir_all(tmp).map_err(at(tmp))?;
    let mut entries = Vec::with_capacity(new.capture.files.len());
    for (index, file) in new.capture.files.iter().enumerate() {
        let name = file
            .source
            .file_name()
            .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
        let stored = format!("files/{}/{index}-{name}", category_name(file.category));
        let target = tmp.join(&stored);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(at(parent))?;
        }
        copy_file(&file.source, &target)?;
        if file_sha256(&target)? != file.sha256 {
            return Err(CloudError::Incomplete(id.to_owned()));
        }
        entries.push(FileEntry {
            category: file.category,
            stored,
            original: file.original.clone(),
            sha256: file.sha256.clone(),
            size: file.size,
        });
    }
    let state_file = match &new.capture.state {
        Some(state) => {
            let path = tmp.join(STATE_FILE);
            write_json_atomic(&path, state)?;
            Some(StoredState {
                sha256: file_sha256(&path)?,
                size: std::fs::metadata(&path).map_err(at(&path))?.len(),
            })
        }
        None => None,
    };
    let state = new.capture.state.as_ref();
    let meta = SnapshotMeta {
        format: FORMAT_VERSION,
        id: id.to_owned(),
        game_key: new.game_key.to_owned(),
        game_name: new.game_name.to_owned(),
        device_id: new.device.id.0.clone(),
        device_name: new.device.name.clone(),
        created_at: new.now,
        session_ended_at: new.session_ended_at,
        parent: new.parent.clone(),
        found_on_device: new.found_on_device,
        kept: new.kept,
        contents: Contents {
            save: entries.iter().any(|e| e.category == Category::Save),
            emulator_files: entries
                .iter()
                .any(|e| e.category == Category::EmulatorFiles),
            state: state.is_some(),
        },
        files: entries,
        digest: new.capture.digest.clone(),
        playtime_seconds: state.map(|s| s.playtime_seconds),
        unlocked: state.map(|s| u32::try_from(s.unlocks.len()).unwrap_or(u32::MAX)),
        local_app_id: new.local_app_id,
        state_file,
    };
    write_json_atomic(&tmp.join(SNAPSHOT_FILE), &meta)?;
    Ok(meta)
}

pub fn read_snapshot(dir: &Path, verify: Verify) -> SnapshotRead {
    let Some(meta) = read_json::<SnapshotMeta>(&dir.join(SNAPSHOT_FILE)) else {
        return SnapshotRead::Incomplete;
    };
    if meta.format > FORMAT_VERSION {
        return SnapshotRead::Incomplete;
    }
    for entry in &meta.files {
        if !is_inside(&entry.stored) {
            return SnapshotRead::Incomplete;
        }
        let path = dir.join(&entry.stored);
        let size_ok = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() == entry.size);
        let hash_ok =
            verify == Verify::Sizes || file_sha256(&path).is_ok_and(|hash| hash == entry.sha256);
        if !size_ok || !hash_ok {
            return SnapshotRead::Incomplete;
        }
    }
    if meta.contents.state && !state_ready(dir, &meta, verify) {
        return SnapshotRead::Incomplete;
    }
    SnapshotRead::Ready(Box::new(meta))
}

fn state_ready(dir: &Path, meta: &SnapshotMeta, verify: Verify) -> bool {
    let path = dir.join(STATE_FILE);
    let Ok(file) = std::fs::metadata(&path) else {
        return false;
    };
    let size_ok = file.is_file()
        && meta
            .state_file
            .as_ref()
            .map_or(file.len() > 0, |stored| stored.size == file.len());
    if !size_ok || verify == Verify::Sizes {
        return size_ok;
    }
    let hash_ok = meta
        .state_file
        .as_ref()
        .is_none_or(|stored| file_sha256(&path).is_ok_and(|hash| hash == stored.sha256));
    hash_ok && load_state(dir).is_ok()
}

fn is_inside(stored: &str) -> bool {
    let path = Path::new(stored);
    !stored.is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

pub fn list_snapshots(games: &[GamePaths]) -> Vec<(PathBuf, SnapshotMeta)> {
    let mut found: Vec<(PathBuf, SnapshotMeta)> = games
        .iter()
        .flat_map(|game| {
            std::fs::read_dir(game.snapshots_dir())
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_dir()
                        && !path
                            .file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with(TMP_PREFIX))
                })
                .collect::<Vec<_>>()
        })
        .filter_map(|dir| match read_snapshot(&dir, Verify::Sizes) {
            SnapshotRead::Ready(meta) => Some((dir, *meta)),
            SnapshotRead::Incomplete => None,
        })
        .collect();
    found.sort_by(|(_, a), (_, b)| (a.session_ended_at, &a.id).cmp(&(b.session_ended_at, &b.id)));
    found
}

pub fn load_state(dir: &Path) -> CloudResult<Option<GameState>> {
    let path = dir.join(STATE_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&std::fs::read(path)?)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::capture::{HashCache, capture};
    use crate::cloud::device::DeviceId;
    use crate::cloud::locate::SaveRoot;
    use crate::cloud::portable::{Placeholders, PortablePath};
    use crate::discovery::folders::KnownFolders;
    use crate::store::UnlockState;

    struct Fixture {
        dir: tempfile::TempDir,
        ph: Placeholders,
        game: GamePaths,
        device: LocalDevice,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let ph = Placeholders {
            folders: Some(KnownFolders::under(&dir.path().join("pc"))),
            ..Default::default()
        };
        let game = GamePaths(dir.path().join("cloud").join("games").join("7 - G"));
        let device = LocalDevice {
            id: DeviceId("dev1".into()),
            name: "Desktop".into(),
        };
        Fixture {
            dir,
            ph,
            game,
            device,
        }
    }

    fn save_file(f: &Fixture, name: &str, text: &str) -> PathBuf {
        let path = f
            .dir
            .path()
            .join("pc")
            .join("RoamingAppData")
            .join("Game")
            .join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    fn roots() -> Vec<SaveRoot> {
        vec![SaveRoot {
            category: Category::Save,
            pattern: PortablePath::new("<winAppData>/Game").unwrap(),
            single_file: false,
        }]
    }

    fn state() -> GameState {
        GameState {
            unlocks: vec![UnlockState {
                api_name: "A".into(),
                unlocked_at: 1,
                detected_at: 1,
                source_id: "gse".into(),
                playtime_seconds: None,
            }],
            progress: Vec::new(),
            playtime_seconds: 3600,
            last_played: Some(5),
        }
    }

    fn write(f: &Fixture, capture: &Capture, now: i64, parent: Option<String>) -> SnapshotMeta {
        write_snapshot(&NewSnapshot {
            game: &f.game,
            game_key: "7",
            game_name: "G",
            device: &f.device,
            now,
            session_ended_at: now,
            parent,
            found_on_device: false,
            kept: false,
            local_app_id: 7,
            capture,
        })
        .unwrap()
    }

    fn dir_of(f: &Fixture, meta: &SnapshotMeta) -> PathBuf {
        list_snapshots(std::slice::from_ref(&f.game))
            .into_iter()
            .find(|(_, m)| m.id == meta.id)
            .unwrap()
            .0
    }

    #[test]
    fn snapshot_round_trips_with_state() {
        let f = fixture();
        save_file(&f, "a.sav", "hello");
        let cap = capture(&roots(), &f.ph, Some(state()), &HashCache::default()).unwrap();
        let meta = write(&f, &cap, 1_793_050_200, Some("p".into()));
        assert_eq!(meta.format, FORMAT_VERSION);
        assert_eq!(meta.digest, cap.digest);
        assert_eq!(meta.playtime_seconds, Some(3600));
        assert_eq!(meta.unlocked, Some(1));
        assert_eq!(
            meta.contents,
            Contents {
                save: true,
                emulator_files: false,
                state: true
            }
        );
        let dir = dir_of(&f, &meta);
        assert!(
            dir.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("2026-10-26_21-30_desktop_")
        );
        match read_snapshot(&dir, Verify::Hashes) {
            SnapshotRead::Ready(read) => assert_eq!(*read, meta),
            SnapshotRead::Incomplete => panic!("incomplete"),
        }
        assert_eq!(load_state(&dir).unwrap(), Some(state()));
        let stored = dir.join(&meta.files[0].stored);
        assert_eq!(std::fs::read_to_string(stored).unwrap(), "hello");
    }

    #[test]
    fn missing_or_changed_file_makes_snapshot_incomplete() {
        let f = fixture();
        save_file(&f, "a.sav", "hello");
        let cap = capture(&roots(), &f.ph, None, &HashCache::default()).unwrap();
        let meta = write(&f, &cap, 1, None);
        let dir = dir_of(&f, &meta);
        let stored = dir.join(&meta.files[0].stored);
        std::fs::write(&stored, "HELLO").unwrap();
        assert!(matches!(
            read_snapshot(&dir, Verify::Sizes),
            SnapshotRead::Ready(_)
        ));
        assert!(matches!(
            read_snapshot(&dir, Verify::Hashes),
            SnapshotRead::Incomplete
        ));
        std::fs::remove_file(&stored).unwrap();
        assert!(matches!(
            read_snapshot(&dir, Verify::Sizes),
            SnapshotRead::Incomplete
        ));
    }

    #[test]
    fn corrupt_snapshot_json_is_incomplete() {
        let f = fixture();
        save_file(&f, "a.sav", "x");
        let cap = capture(&roots(), &f.ph, None, &HashCache::default()).unwrap();
        let meta = write(&f, &cap, 1, None);
        let dir = dir_of(&f, &meta);
        std::fs::write(dir.join(SNAPSHOT_FILE), "{ nope").unwrap();
        assert!(matches!(
            read_snapshot(&dir, Verify::Sizes),
            SnapshotRead::Incomplete
        ));
        assert!(list_snapshots(std::slice::from_ref(&f.game)).is_empty());
    }

    #[test]
    fn newer_format_is_incomplete() {
        let f = fixture();
        save_file(&f, "a.sav", "x");
        let cap = capture(&roots(), &f.ph, None, &HashCache::default()).unwrap();
        let mut meta = write(&f, &cap, 1, None);
        let dir = dir_of(&f, &meta);
        meta.format = FORMAT_VERSION + 1;
        std::fs::write(dir.join(SNAPSHOT_FILE), serde_json::to_vec(&meta).unwrap()).unwrap();
        assert!(matches!(
            read_snapshot(&dir, Verify::Sizes),
            SnapshotRead::Incomplete
        ));
    }

    #[test]
    fn failed_write_leaves_no_snapshot_and_no_tmp() {
        let f = fixture();
        let path = save_file(&f, "a.sav", "x");
        let cap = capture(&roots(), &f.ph, None, &HashCache::default()).unwrap();
        std::fs::remove_file(path).unwrap();
        let result = write_snapshot(&NewSnapshot {
            game: &f.game,
            game_key: "7",
            game_name: "G",
            device: &f.device,
            now: 1,
            session_ended_at: 1,
            parent: None,
            found_on_device: false,
            kept: false,
            local_app_id: 7,
            capture: &cap,
        });
        assert!(result.is_err());
        let left: Vec<_> = std::fs::read_dir(f.game.snapshots_dir())
            .map(|entries| entries.flatten().collect())
            .unwrap_or_default();
        assert!(left.is_empty());
    }

    #[test]
    fn list_skips_tmp_folders_and_sorts_by_session_end() {
        let f = fixture();
        save_file(&f, "a.sav", "x");
        let cap = capture(&roots(), &f.ph, None, &HashCache::default()).unwrap();
        let later = write(&f, &cap, 200, None);
        let earlier = write(&f, &cap, 100, None);
        std::fs::create_dir_all(f.game.snapshots_dir().join(".tmp-zzz")).unwrap();
        let ids: Vec<String> = list_snapshots(std::slice::from_ref(&f.game))
            .into_iter()
            .map(|(_, m)| m.id)
            .collect();
        assert_eq!(ids, vec![earlier.id, later.id]);
    }

    #[test]
    fn snapshot_without_state_has_no_state_file() {
        let f = fixture();
        save_file(&f, "a.sav", "x");
        let cap = capture(&roots(), &f.ph, None, &HashCache::default()).unwrap();
        let meta = write(&f, &cap, 1, None);
        assert_eq!(load_state(&dir_of(&f, &meta)).unwrap(), None);
        assert!(!meta.contents.state);
        assert_eq!(meta.playtime_seconds, None);
    }
}
