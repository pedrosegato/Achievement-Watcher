use std::io;

use serde::{Deserialize, Serialize};

use crate::cloud::device::DeviceId;
use crate::cloud::layout::{GamePaths, read_json, write_json_atomic};

pub const LOCK_RENEW_SECS: i64 = 120;
pub const LOCK_STALE_SECS: i64 = 600;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockRecord {
    pub device_id: String,
    pub device_name: String,
    pub game_key: String,
    pub started_at: i64,
    pub renewed_at: i64,
}

pub fn write_lock(game: &GamePaths, device: &DeviceId, record: &LockRecord) -> io::Result<()> {
    write_json_atomic(&game.lock_file(device), record)
}

pub fn remove_own_lock(game: &GamePaths, device: &DeviceId) -> io::Result<()> {
    match std::fs::remove_file(game.lock_file(device)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

pub fn own_lock(game: &GamePaths, device: &DeviceId) -> Option<LockRecord> {
    read_json(&game.lock_file(device))
}

pub fn foreign_locks(games: &[GamePaths], own: &DeviceId, now: i64) -> Vec<LockRecord> {
    let mut locks: Vec<LockRecord> = games
        .iter()
        .flat_map(|game| {
            std::fs::read_dir(game.locks_dir())
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .collect::<Vec<_>>()
        })
        .filter_map(|path| read_json::<LockRecord>(&path))
        .filter(|lock| lock.device_id != own.0 && now - lock.renewed_at <= LOCK_STALE_SECS)
        .collect();
    locks.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    locks.dedup_by(|a, b| a.device_id == b.device_id);
    locks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(device: &str, renewed_at: i64) -> LockRecord {
        LockRecord {
            device_id: device.into(),
            device_name: device.to_uppercase(),
            game_key: "7".into(),
            started_at: renewed_at,
            renewed_at,
        }
    }

    #[test]
    fn stale_foreign_lock_is_ignored_and_own_lock_is_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let game = GamePaths(dir.path().to_path_buf());
        let own = DeviceId("me".into());
        let now = 10_000;
        write_lock(&game, &own, &record("me", now)).unwrap();
        write_lock(
            &game,
            &DeviceId("fresh".into()),
            &record("fresh", now - LOCK_STALE_SECS),
        )
        .unwrap();
        write_lock(
            &game,
            &DeviceId("old".into()),
            &record("old", now - LOCK_STALE_SECS - 1),
        )
        .unwrap();
        let locks = foreign_locks(std::slice::from_ref(&game), &own, now);
        let names: Vec<&str> = locks.iter().map(|l| l.device_id.as_str()).collect();
        assert_eq!(names, vec!["fresh"]);
    }

    #[test]
    fn own_lock_round_trips_and_removing_keeps_others() {
        let dir = tempfile::tempdir().unwrap();
        let game = GamePaths(dir.path().to_path_buf());
        let me = DeviceId("me".into());
        let other = DeviceId("other".into());
        assert_eq!(own_lock(&game, &me), None);
        write_lock(&game, &me, &record("me", 5)).unwrap();
        write_lock(&game, &other, &record("other", 5)).unwrap();
        assert_eq!(own_lock(&game, &me), Some(record("me", 5)));
        remove_own_lock(&game, &me).unwrap();
        remove_own_lock(&game, &me).unwrap();
        assert_eq!(own_lock(&game, &me), None);
        assert_eq!(own_lock(&game, &other), Some(record("other", 5)));
    }
}
