use std::io;
use std::path::Path;

use crate::cloud::device::{DeviceId, LocalDevice};
use crate::cloud::heads::{HeadChoice, write_head};
use crate::cloud::keyed::KeyedDoc;
use crate::cloud::layout::{CloudLayout, GamePaths, read_json, write_json_atomic};
use crate::cloud::lock::{LockRecord, write_lock};

pub fn adopt_former_ids(layout: &CloudLayout, device: &LocalDevice) -> io::Result<()> {
    if device.former_ids.is_empty() || !layout.exists() {
        return Ok(());
    }
    let games: Vec<GamePaths> = layout
        .games()
        .into_iter()
        .flat_map(|(_, dirs)| dirs)
        .collect();
    for former in &device.former_ids {
        for game in &games {
            move_head(game, former, &device.id)?;
            move_lock(game, former, &device.id)?;
        }
        move_settings(layout, former, &device.id)?;
        remove_if_present(&layout.device_file(former))?;
    }
    Ok(())
}

fn move_head(game: &GamePaths, former: &DeviceId, id: &DeviceId) -> io::Result<()> {
    let old_path = game.head_file(former);
    let Some(mut old) = read_json::<HeadChoice>(&old_path) else {
        return Ok(());
    };
    let newer_here = read_json::<HeadChoice>(&game.head_file(id))
        .is_some_and(|current| current.chosen_at >= old.chosen_at);
    if !newer_here {
        old.device_id = id.0.clone();
        write_head(game, id, &old)?;
    }
    remove_if_present(&old_path)
}

fn move_lock(game: &GamePaths, former: &DeviceId, id: &DeviceId) -> io::Result<()> {
    let old_path = game.lock_file(former);
    let Some(mut old) = read_json::<LockRecord>(&old_path) else {
        return Ok(());
    };
    if !game.lock_file(id).exists() {
        old.device_id = id.0.clone();
        write_lock(game, id, &old)?;
    }
    remove_if_present(&old_path)
}

fn move_settings(layout: &CloudLayout, former: &DeviceId, id: &DeviceId) -> io::Result<()> {
    let old_path = layout.settings_file(former);
    let Some(old) = read_json::<KeyedDoc>(&old_path) else {
        return Ok(());
    };
    let path = layout.settings_file(id);
    let mut doc = read_json::<KeyedDoc>(&path).unwrap_or_default();
    doc.device_id = id.0.clone();
    for (key, stamped) in old.keys {
        let newer_here = doc.keys.get(&key).is_some_and(|kept| kept.at >= stamped.at);
        if !newer_here {
            doc.keys.insert(key, stamped);
        }
    }
    write_json_atomic(&path, &doc)?;
    remove_if_present(&old_path)
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::cloud::device::DeviceId;
    use crate::cloud::heads::{HeadChoice, HeadReason, write_head};
    use crate::cloud::key::CloudKey;
    use crate::cloud::keyed::{KeyedDoc, Stamped};
    use crate::cloud::layout::{GamePaths, read_json, write_json_atomic};
    use crate::cloud::lock::{LockRecord, write_lock};

    fn device() -> LocalDevice {
        let mut device = LocalDevice::new(DeviceId("new".into()), "Desktop");
        device.former_ids.push(DeviceId("old".into()));
        device
    }

    fn head(device: &str, chosen_at: i64, snapshot: &str) -> HeadChoice {
        HeadChoice {
            snapshot_id: snapshot.into(),
            previous: None,
            reason: HeadReason::Push,
            chosen_at,
            device_id: device.into(),
            device_name: "Desktop".into(),
        }
    }

    fn doc(device: &str, key: &str, at: i64) -> KeyedDoc {
        KeyedDoc {
            device_id: device.into(),
            keys: BTreeMap::from([(
                key.to_owned(),
                Stamped {
                    value: serde_json::json!(true),
                    at,
                },
            )]),
        }
    }

    fn setup() -> (tempfile::TempDir, CloudLayout, GamePaths) {
        let dir = tempfile::tempdir().unwrap();
        let layout = CloudLayout::new(dir.path().to_path_buf());
        let game = layout
            .game_dir_for_write(&CloudKey::Steam(1_245_620), "ELDEN RING")
            .unwrap();
        (dir, layout, game)
    }

    #[test]
    fn head_moves_to_the_new_id() {
        let (_dir, layout, game) = setup();
        let old = DeviceId("old".into());
        write_head(&game, &old, &head("old", 10, "a")).unwrap();
        adopt_former_ids(&layout, &device()).unwrap();
        assert!(!game.head_file(&old).exists());
        let moved = read_json::<HeadChoice>(&game.head_file(&DeviceId("new".into()))).unwrap();
        assert_eq!(moved, head("new", 10, "a"));
    }

    #[test]
    fn newer_head_wins_when_both_ids_have_one() {
        let (_dir, layout, game) = setup();
        let new = DeviceId("new".into());
        write_head(
            &game,
            &DeviceId("old".into()),
            &head("old", 30, "old-choice"),
        )
        .unwrap();
        write_head(&game, &new, &head("new", 20, "new-choice")).unwrap();
        adopt_former_ids(&layout, &device()).unwrap();
        let kept = read_json::<HeadChoice>(&game.head_file(&new)).unwrap();
        assert_eq!(kept.snapshot_id, "old-choice");
        assert_eq!(kept.device_id, "new");

        write_head(&game, &DeviceId("old".into()), &head("old", 5, "stale")).unwrap();
        adopt_former_ids(&layout, &device()).unwrap();
        let kept = read_json::<HeadChoice>(&game.head_file(&new)).unwrap();
        assert_eq!(kept.snapshot_id, "old-choice");
        assert!(!game.head_file(&DeviceId("old".into())).exists());
    }

    #[test]
    fn lock_moves_to_the_new_id() {
        let (_dir, layout, game) = setup();
        let old = DeviceId("old".into());
        let record = LockRecord {
            device_id: "old".into(),
            device_name: "Desktop".into(),
            game_key: "1245620".into(),
            started_at: 1,
            renewed_at: 2,
        };
        write_lock(&game, &old, &record).unwrap();
        adopt_former_ids(&layout, &device()).unwrap();
        assert!(!game.lock_file(&old).exists());
        let moved = read_json::<LockRecord>(&game.lock_file(&DeviceId("new".into()))).unwrap();
        assert_eq!(moved.device_id, "new");
        assert_eq!(moved.renewed_at, 2);
    }

    #[test]
    fn settings_doc_moves_and_merges_by_stamp() {
        let (_dir, layout, _game) = setup();
        let old = DeviceId("old".into());
        let new = DeviceId("new".into());
        let mut old_doc = doc("old", "language", 50);
        old_doc.keys.insert(
            "theme".into(),
            Stamped {
                value: serde_json::json!("dark"),
                at: 10,
            },
        );
        write_json_atomic(&layout.settings_file(&old), &old_doc).unwrap();
        let mut new_doc = doc("new", "language", 40);
        new_doc.keys.insert(
            "theme".into(),
            Stamped {
                value: serde_json::json!("light"),
                at: 90,
            },
        );
        write_json_atomic(&layout.settings_file(&new), &new_doc).unwrap();
        adopt_former_ids(&layout, &device()).unwrap();
        assert!(!layout.settings_file(&old).exists());
        let merged = read_json::<KeyedDoc>(&layout.settings_file(&new)).unwrap();
        assert_eq!(merged.device_id, "new");
        assert_eq!(merged.keys["language"].at, 50);
        assert_eq!(merged.keys["theme"].value, serde_json::json!("light"));
    }

    #[test]
    fn old_device_record_is_removed_and_reruns_are_harmless() {
        let (_dir, layout, _game) = setup();
        let old = DeviceId("old".into());
        write_json_atomic(
            &layout.device_file(&old),
            &serde_json::json!({"name": "pedro pc"}),
        )
        .unwrap();
        adopt_former_ids(&layout, &device()).unwrap();
        assert!(!layout.device_file(&old).exists());
        adopt_former_ids(&layout, &device()).unwrap();
        let empty = tempfile::tempdir().unwrap();
        adopt_former_ids(&CloudLayout::new(empty.path().join("missing")), &device()).unwrap();
    }
}
