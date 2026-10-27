use std::collections::HashSet;
use std::io;

use serde::{Deserialize, Serialize};

use crate::cloud::device::DeviceId;
use crate::cloud::layout::{GamePaths, read_json, write_json_atomic};
use crate::cloud::snapshot::SnapshotMeta;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeadReason {
    Push,
    Restore,
    Branch,
    Manual,
    Undo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadChoice {
    pub snapshot_id: String,
    pub previous: Option<String>,
    pub reason: HeadReason,
    pub chosen_at: i64,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Principal {
    pub snapshot_id: String,
    pub choice: Option<HeadChoice>,
}

pub fn read_heads(games: &[GamePaths]) -> Vec<HeadChoice> {
    let mut heads: Vec<HeadChoice> = games
        .iter()
        .flat_map(|game| {
            std::fs::read_dir(game.heads_dir())
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .collect::<Vec<_>>()
        })
        .filter_map(|path| read_json::<HeadChoice>(&path))
        .collect();
    heads.sort_by(|a, b| (a.chosen_at, &a.device_id).cmp(&(b.chosen_at, &b.device_id)));
    heads
}

pub fn write_head(game: &GamePaths, device: &DeviceId, choice: &HeadChoice) -> io::Result<()> {
    write_json_atomic(&game.head_file(device), choice)
}

pub fn principal(heads: &[HeadChoice], snapshots: &[SnapshotMeta]) -> Option<Principal> {
    let newest = newest(snapshots.iter())?;
    let Some(latest) = heads
        .iter()
        .max_by(|a, b| (a.chosen_at, &a.device_id).cmp(&(b.chosen_at, &b.device_id)))
    else {
        return Some(Principal {
            snapshot_id: newest.id.clone(),
            choice: None,
        });
    };
    let exists = |id: &str| snapshots.iter().any(|s| s.id == id);
    let snapshot_id = if exists(&latest.snapshot_id) {
        latest.snapshot_id.clone()
    } else if let Some(found) = newest_descendant(&latest.snapshot_id, snapshots) {
        found.id.clone()
    } else if let Some(previous) = latest.previous.as_deref().filter(|id| exists(id)) {
        previous.to_owned()
    } else {
        newest.id.clone()
    };
    Some(Principal {
        snapshot_id,
        choice: Some(latest.clone()),
    })
}

fn newest<'a>(snapshots: impl Iterator<Item = &'a SnapshotMeta>) -> Option<&'a SnapshotMeta> {
    snapshots.max_by(|a, b| (a.session_ended_at, &a.id).cmp(&(b.session_ended_at, &b.id)))
}

fn newest_descendant<'a>(root: &str, snapshots: &'a [SnapshotMeta]) -> Option<&'a SnapshotMeta> {
    let mut family: HashSet<&str> = HashSet::from([root]);
    loop {
        let before = family.len();
        for snapshot in snapshots {
            if snapshot
                .parent
                .as_deref()
                .is_some_and(|parent| family.contains(parent))
            {
                family.insert(&snapshot.id);
            }
        }
        if family.len() == before {
            break;
        }
    }
    newest(
        snapshots
            .iter()
            .filter(|s| s.id != root && family.contains(s.id.as_str())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::snapshot::{Contents, FORMAT_VERSION};

    fn snap(id: &str, parent: Option<&str>, ended: i64) -> SnapshotMeta {
        SnapshotMeta {
            format: FORMAT_VERSION,
            id: id.into(),
            game_key: "7".into(),
            game_name: "G".into(),
            device_id: "d".into(),
            device_name: "D".into(),
            created_at: ended,
            session_ended_at: ended,
            parent: parent.map(str::to_owned),
            found_on_device: false,
            kept: false,
            contents: Contents::default(),
            files: Vec::new(),
            digest: id.into(),
            playtime_seconds: None,
            unlocked: None,
            local_app_id: 7,
            state_file: None,
            replaces: None,
        }
    }

    fn choice(id: &str, previous: Option<&str>, at: i64, device: &str) -> HeadChoice {
        HeadChoice {
            snapshot_id: id.into(),
            previous: previous.map(str::to_owned),
            reason: HeadReason::Push,
            chosen_at: at,
            device_id: device.into(),
            device_name: device.into(),
        }
    }

    #[test]
    fn latest_choice_wins() {
        let snaps = [snap("a", None, 1), snap("b", Some("a"), 2)];
        let heads = [choice("b", None, 10, "x"), choice("a", None, 20, "y")];
        assert_eq!(principal(&heads, &snaps).unwrap().snapshot_id, "a");
    }

    #[test]
    fn tie_breaks_by_device_id() {
        let snaps = [snap("a", None, 1), snap("b", None, 2)];
        let heads = [choice("a", None, 10, "x"), choice("b", None, 10, "y")];
        assert_eq!(principal(&heads, &snaps).unwrap().snapshot_id, "b");
    }

    #[test]
    fn deleted_principal_falls_back_to_newest_descendant() {
        let snaps = [
            snap("a", None, 1),
            snap("c", Some("b"), 3),
            snap("d", Some("c"), 4),
            snap("e", Some("a"), 5),
        ];
        let heads = [choice("b", Some("a"), 10, "x")];
        assert_eq!(principal(&heads, &snaps).unwrap().snapshot_id, "d");
    }

    #[test]
    fn deleted_principal_without_descendants_uses_previous_then_newest() {
        let snaps = [snap("a", None, 1), snap("e", None, 5)];
        let heads = [choice("b", Some("a"), 10, "x")];
        assert_eq!(principal(&heads, &snaps).unwrap().snapshot_id, "a");
        let heads = [choice("b", Some("gone"), 10, "x")];
        assert_eq!(principal(&heads, &snaps).unwrap().snapshot_id, "e");
    }

    #[test]
    fn no_heads_means_newest_snapshot() {
        let snaps = [snap("a", None, 1), snap("b", None, 2)];
        let got = principal(&[], &snaps).unwrap();
        assert_eq!(got.snapshot_id, "b");
        assert_eq!(got.choice, None);
        assert_eq!(principal(&[], &[]), None);
    }

    #[test]
    fn heads_round_trip_and_corrupt_head_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let game = GamePaths(dir.path().to_path_buf());
        let device = DeviceId("x".into());
        write_head(&game, &device, &choice("a", None, 1, "x")).unwrap();
        std::fs::write(game.heads_dir().join("broken.json"), "{").unwrap();
        let heads = read_heads(std::slice::from_ref(&game));
        assert_eq!(heads, vec![choice("a", None, 1, "x")]);
    }

    #[test]
    fn conflict_copy_head_file_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let game = GamePaths(dir.path().to_path_buf());
        write_head(&game, &DeviceId("x".into()), &choice("a", None, 1, "x")).unwrap();
        std::fs::copy(
            game.heads_dir().join("x.json"),
            game.heads_dir().join("x (1).json"),
        )
        .unwrap();
        assert_eq!(read_heads(std::slice::from_ref(&game)).len(), 2);
    }
}
