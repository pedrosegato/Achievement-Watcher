use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::discovery::detect::{DetectConfig, detect_in_game_folders, merge_found};
use crate::discovery::folders::{Base, KnownFolders};
use crate::discovery::gamediscovery::Unresolved;
use crate::discovery::running::KnownGame;
use crate::discovery::sources::builtin_sources;
use crate::discovery::{Found, discover};
use crate::fileio::{fnv1a64, read_with_retry};
use crate::model::Unlock;
use crate::model::sane_unlock_time;
use crate::parsers::parse_file;
use crate::parsers::smartsteam::{CRC_PREFIX, crc_name, crc32};
use crate::store::{Store, StoreResult};

const FIRST_BOOT_KEY: &str = "first_boot_done";
const READ_BUDGET: Duration = Duration::from_secs(1);
const APPROXIMATE_SOURCES: [(&str, i64); 1] = [("creamapi", 999)];
const ALL_BASES: [(Base, &str); 5] = [
    (Base::PublicDocuments, "PublicDocuments"),
    (Base::RoamingAppData, "RoamingAppData"),
    (Base::ProgramData, "ProgramData"),
    (Base::LocalAppData, "LocalAppData"),
    (Base::Documents, "Documents"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreSettings {
    pub silent_after_minutes: i64,
    pub detect: DetectConfig,
}

impl Default for CoreSettings {
    fn default() -> CoreSettings {
        CoreSettings {
            silent_after_minutes: 5,
            detect: DetectConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewUnlock {
    pub app_id: u32,
    pub api_name: String,
    pub unlocked_at: i64,
    pub source_id: &'static str,
    pub notify: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgressUpdate {
    pub app_id: u32,
    pub api_name: String,
    pub current: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SyncOutcome {
    pub first_boot: bool,
    pub new_unlocks: Vec<NewUnlock>,
    pub progress: Vec<ProgressUpdate>,
    pub games_seen: usize,
    pub relocked: usize,
    pub files_failed: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatchTargets {
    pub recursive: Vec<PathBuf>,
    pub flat: Vec<PathBuf>,
}

pub struct Engine {
    store: Store,
    folders: KnownFolders,
    settings: CoreSettings,
    fixed_found: Vec<Found>,
    game_found: Vec<Found>,
    unresolved: Vec<Unresolved>,
    known_games: Vec<KnownGame>,
}

impl Engine {
    pub fn new(store: Store, folders: KnownFolders, settings: CoreSettings) -> Engine {
        for (base, name) in ALL_BASES {
            if folders.resolve(base).is_none() {
                tracing::warn!("known folder {name} is unavailable and will be skipped");
            }
        }
        Engine {
            store,
            folders,
            settings,
            fixed_found: Vec::new(),
            game_found: Vec::new(),
            unresolved: Vec::new(),
            known_games: Vec::new(),
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn settings(&self) -> &CoreSettings {
        &self.settings
    }

    pub fn set_settings(&mut self, settings: CoreSettings) {
        self.settings = settings;
    }

    pub fn found(&self) -> Vec<Found> {
        merge_found(self.fixed_found.clone(), self.game_found.clone())
    }

    pub fn unresolved(&self) -> &[Unresolved] {
        &self.unresolved
    }

    pub fn known_games(&self) -> &[KnownGame] {
        &self.known_games
    }

    pub fn watch_targets(&self) -> WatchTargets {
        let mut recursive = BTreeSet::new();
        let mut flat = BTreeSet::new();
        for source in builtin_sources() {
            let Some(base) = self.folders.resolve(source.base) else {
                continue;
            };
            let root = source
                .root
                .split('/')
                .fold(base.to_path_buf(), |dir, segment| dir.join(segment));
            if root.is_dir() {
                recursive.insert(root);
            } else if let Some(existing) = nearest_existing(&root) {
                flat.insert(existing);
            }
        }
        for found in &self.game_found {
            if let Some(parent) = found.path.parent() {
                flat.insert(parent.to_path_buf());
            }
        }
        let recursive: Vec<PathBuf> = recursive.into_iter().collect();
        let flat = flat
            .into_iter()
            .filter(|dir| !recursive.iter().any(|root| dir.starts_with(root)))
            .collect();
        WatchTargets { recursive, flat }
    }

    pub fn sync(&mut self, now: i64, full: bool) -> StoreResult<SyncOutcome> {
        self.sync_reporting(now, full, &mut |_, _| {})
    }

    pub fn sync_reporting(
        &mut self,
        now: i64,
        full: bool,
        progress: &mut dyn FnMut(usize, usize),
    ) -> StoreResult<SyncOutcome> {
        self.fixed_found = discover(builtin_sources(), &self.folders);
        if full {
            let in_games = detect_in_game_folders(&self.folders, &self.settings.detect);
            self.game_found = in_games.found;
            self.unresolved = in_games.unresolved;
            self.known_games = in_games.games;
        }
        let found = self.found();
        let first_boot = self.store.meta_get(FIRST_BOOT_KEY)?.is_none();
        let mut outcome = SyncOutcome {
            first_boot,
            games_seen: found
                .iter()
                .map(|f| f.app_id)
                .collect::<BTreeSet<_>>()
                .len(),
            ..SyncOutcome::default()
        };
        let total = found.len();
        progress(0, total);
        let tx = self.store.begin()?;
        for (index, file) in found.iter().enumerate() {
            self.store.ensure_game(file.app_id)?;
            self.sync_file(file, now, &mut outcome)?;
            progress(index + 1, total);
        }
        if first_boot {
            self.store.meta_set(FIRST_BOOT_KEY, &now.to_string())?;
        }
        tx.commit()?;
        let silent_window = self.settings.silent_after_minutes.max(0) * 60;
        for unlock in &mut outcome.new_unlocks {
            let slack = approximate_slack(unlock.source_id);
            unlock.notify = !first_boot && now - (unlock.unlocked_at + slack) <= silent_window;
        }
        outcome.new_unlocks.sort_by(|a, b| {
            (a.unlocked_at, a.app_id, &a.api_name).cmp(&(b.unlocked_at, b.app_id, &b.api_name))
        });
        Ok(outcome)
    }

    fn sync_file(&self, file: &Found, now: i64, outcome: &mut SyncOutcome) -> StoreResult<()> {
        let path = file.path.to_string_lossy();
        let bytes = match read_with_retry(&file.path, READ_BUDGET) {
            Ok(bytes) => bytes,
            Err(error) => {
                outcome.files_failed += 1;
                tracing::warn!("cannot read {path}: {error}");
                return self.store.mark_file_error(
                    &path,
                    file.source_id,
                    file.app_id,
                    &error.to_string(),
                );
            }
        };
        let hash = format!("{:016x}", fnv1a64(&bytes));
        if self.store.file_hash(&path)?.as_deref() == Some(hash.as_str()) {
            return Ok(());
        }
        let parsed = parse_file(file.format, &file.path, &bytes)
            .map_err(|error| error.to_string())
            .and_then(|unlocks| self.resolve_crc_names(file, unlocks));
        let unlocks = match parsed {
            Ok(unlocks) => unlocks,
            Err(error) => {
                outcome.files_failed += 1;
                tracing::warn!("cannot parse {path}: {error}");
                return self
                    .store
                    .mark_file_error(&path, file.source_id, file.app_id, &error);
            }
        };
        for unlock in &unlocks {
            if unlock.unlocked {
                let time = unlock
                    .unlocked_at
                    .and_then(|raw| sane_unlock_time(raw, now))
                    .unwrap_or(now);
                if self.store.record_unlock(
                    file.app_id,
                    &unlock.api_name,
                    time,
                    now,
                    file.source_id,
                )? {
                    outcome.new_unlocks.push(NewUnlock {
                        app_id: file.app_id,
                        api_name: unlock.api_name.clone(),
                        unlocked_at: time,
                        source_id: file.source_id,
                        notify: false,
                    });
                }
            } else if let Some(progress) = unlock
                .progress
                .as_ref()
                .filter(|p| p.max > 0 && p.current < p.max)
            {
                let previous = self.store.update_progress(
                    file.app_id,
                    &unlock.api_name,
                    progress.current,
                    progress.max,
                )?;
                if !outcome.first_boot
                    && previous.is_some_and(|_| {
                        crosses_milestone(previous, progress.current, progress.max)
                    })
                {
                    outcome.progress.push(ProgressUpdate {
                        app_id: file.app_id,
                        api_name: unlock.api_name.clone(),
                        current: progress.current,
                        max: progress.max,
                    });
                }
            }
            if !unlock.unlocked && self.store.is_unlocked(file.app_id, &unlock.api_name)? {
                outcome.relocked += 1;
                tracing::info!(
                    "{} {} is locked again in {path}; keeping the recorded unlock",
                    file.app_id,
                    unlock.api_name
                );
            }
        }
        let count = u32::try_from(unlocks.len()).unwrap_or(u32::MAX);
        self.store
            .mark_file_ok(&path, file.source_id, file.app_id, &hash, count, now)
    }
}

impl Engine {
    fn resolve_crc_names(&self, file: &Found, unlocks: Vec<Unlock>) -> Result<Vec<Unlock>, String> {
        if !unlocks.iter().any(|u| u.api_name.starts_with(CRC_PREFIX)) {
            return Ok(unlocks);
        }
        let names: HashMap<String, String> = self
            .store
            .schema_names(file.app_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|name| (crc_name(crc32(name.as_bytes())), name))
            .collect();
        unlocks
            .into_iter()
            .map(|mut unlock| {
                if unlock.api_name.starts_with(CRC_PREFIX) {
                    unlock.api_name = names
                        .get(&unlock.api_name)
                        .cloned()
                        .ok_or_else(|| "achievement names not available yet".to_owned())?;
                }
                Ok(unlock)
            })
            .collect()
    }
}

const MILESTONES: [u64; 3] = [25, 50, 75];

pub fn crosses_milestone(previous: Option<u32>, current: u32, max: u32) -> bool {
    let before = previous.unwrap_or(0);
    if current <= before || current >= max {
        return false;
    }
    if max > 1 && current == max - 1 {
        return true;
    }
    let (before, current, max) = (u64::from(before), u64::from(current), u64::from(max));
    MILESTONES
        .iter()
        .any(|pct| before * 100 < pct * max && pct * max <= current * 100)
}

fn approximate_slack(source_id: &str) -> i64 {
    APPROXIMATE_SOURCES
        .iter()
        .find(|(id, _)| *id == source_id)
        .map_or(0, |(_, slack)| *slack)
}

fn nearest_existing(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .skip(1)
        .find(|ancestor| ancestor.is_dir())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn touch(root: &Path, rel: &str, body: &str) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
        p
    }

    fn engine(root: &Path) -> Engine {
        Engine::new(
            Store::in_memory().unwrap(),
            KnownFolders::under(root),
            CoreSettings::default(),
        )
    }

    fn onlinefix(root: &Path, app: u32, body: &str) -> PathBuf {
        touch(
            root,
            &format!("PublicDocuments/OnlineFix/{app}/Stats/Achievements.ini"),
            body,
        )
    }

    #[test]
    fn first_boot_is_silent_with_summary() {
        let tmp = tempfile::tempdir().unwrap();
        onlinefix(
            tmp.path(),
            10,
            &format!(
                "[A]\nachieved=true\ntimestamp={NOW}\n[B]\nachieved=true\ntimestamp=1700000000\n"
            ),
        );
        onlinefix(tmp.path(), 20, "[C]\nachieved=true\n");
        let mut e = engine(tmp.path());
        let out = e.sync(NOW, true).unwrap();
        assert!(out.first_boot);
        assert_eq!(out.new_unlocks.len(), 3);
        assert!(out.new_unlocks.iter().all(|u| !u.notify));
        assert_eq!(out.games_seen, 2);
        let again = e.sync(NOW + 1, true).unwrap();
        assert!(!again.first_boot);
        assert!(again.new_unlocks.is_empty());
    }

    #[test]
    fn sync_reports_file_progress() {
        let tmp = tempfile::tempdir().unwrap();
        for app in [10, 20, 30] {
            onlinefix(
                tmp.path(),
                app,
                "[A]
achieved=true
",
            );
        }
        let mut e = engine(tmp.path());
        let mut seen = Vec::new();
        e.sync_reporting(NOW, true, &mut |done, total| seen.push((done, total)))
            .unwrap();
        assert_eq!(seen, vec![(0, 3), (1, 3), (2, 3), (3, 3)]);
    }

    #[test]
    fn new_unlock_after_first_boot_notifies() {
        let tmp = tempfile::tempdir().unwrap();
        let p = onlinefix(tmp.path(), 10, "[A]\nachieved=false\n");
        let mut e = engine(tmp.path());
        e.sync(NOW, true).unwrap();
        std::fs::write(&p, format!("[A]\nachieved=true\ntimestamp={}\n", NOW + 5)).unwrap();
        let out = e.sync(NOW + 10, false).unwrap();
        assert_eq!(
            out.new_unlocks,
            vec![NewUnlock {
                app_id: 10,
                api_name: "A".into(),
                unlocked_at: NOW + 5,
                source_id: "onlinefix",
                notify: true
            }]
        );
    }

    #[test]
    fn unlock_without_time_uses_detection_time_and_notifies() {
        let tmp = tempfile::tempdir().unwrap();
        let p = onlinefix(tmp.path(), 10, "[A]\nachieved=false\n");
        let mut e = engine(tmp.path());
        e.sync(NOW, true).unwrap();
        std::fs::write(&p, "[A]\nachieved=true\n").unwrap();
        let out = e.sync(NOW + 10, false).unwrap();
        assert_eq!(out.new_unlocks[0].unlocked_at, NOW + 10);
        assert!(out.new_unlocks[0].notify);
    }

    #[test]
    fn old_unlock_after_downtime_is_silent() {
        let tmp = tempfile::tempdir().unwrap();
        let p = onlinefix(tmp.path(), 10, "[A]\nachieved=false\n");
        let mut e = engine(tmp.path());
        e.sync(NOW, true).unwrap();
        std::fs::write(
            &p,
            format!("[A]\nachieved=true\ntimestamp={}\n", NOW - 3600),
        )
        .unwrap();
        let out = e.sync(NOW + 10, false).unwrap();
        assert_eq!(out.new_unlocks.len(), 1);
        assert!(!out.new_unlocks[0].notify);
    }

    #[test]
    fn creamapi_slack_keeps_recent_unlock_notifying() {
        let tmp = tempfile::tempdir().unwrap();
        let rel = "RoamingAppData/CreamAPI/30/stats/CreamAPI.Achievements.cfg";
        let p = touch(tmp.path(), rel, "[A]\nachieved=false\n");
        let mut e = engine(tmp.path());
        e.sync(NOW, true).unwrap();
        let floored = (NOW / 1000) * 1000 - 1000;
        std::fs::write(
            &p,
            format!("[A]\nachieved=true\nunlocktime={}\n", floored / 1000),
        )
        .unwrap();
        let now = floored + 1000 + 250;
        let out = e.sync(now, false).unwrap();
        assert_eq!(out.new_unlocks[0].unlocked_at, floored);
        assert!(now - floored > 5 * 60);
        assert!(out.new_unlocks[0].notify);
    }

    #[test]
    fn unchanged_file_is_not_reparsed() {
        let tmp = tempfile::tempdir().unwrap();
        let p = onlinefix(tmp.path(), 10, "[A]\nachieved=true\n");
        let mut e = engine(tmp.path());
        e.sync(NOW, true).unwrap();
        let before = e.store().files_report().unwrap();
        e.sync(NOW + 100, false).unwrap();
        let after = e.store().files_report().unwrap();
        assert_eq!(before, after);
        assert_eq!(PathBuf::from(&after[0].path), p);
        assert_eq!(after[0].last_ok_at, Some(NOW));
    }

    #[test]
    fn failed_read_is_not_marked() {
        let tmp = tempfile::tempdir().unwrap();
        let p = onlinefix(tmp.path(), 10, "[A]\nachieved=false\n");
        let mut e = engine(tmp.path());
        e.sync(NOW, true).unwrap();
        std::fs::write(&p, "").unwrap();
        let out = e.sync(NOW + 1, false).unwrap();
        assert_eq!(out.files_failed, 1);
        std::fs::write(&p, format!("[A]\nachieved=true\ntimestamp={}\n", NOW + 1)).unwrap();
        let out = e.sync(NOW + 2, false).unwrap();
        assert_eq!(out.new_unlocks.len(), 1);
        std::fs::write(&p, "").unwrap();
        e.sync(NOW + 3, false).unwrap();
        std::fs::write(&p, format!("[A]\nachieved=true\ntimestamp={}\n", NOW + 1)).unwrap();
        let report = e.store().files_report().unwrap();
        assert_eq!(report[0].last_error.as_deref(), Some("empty file"));
        assert_eq!(report[0].last_ok_at, Some(NOW + 2));
    }

    #[test]
    fn relock_is_logged_not_deleted() {
        let tmp = tempfile::tempdir().unwrap();
        let p = onlinefix(tmp.path(), 10, "[A]\nachieved=true\n");
        let mut e = engine(tmp.path());
        e.sync(NOW, true).unwrap();
        std::fs::write(&p, "[A]\nachieved=false\n").unwrap();
        let out = e.sync(NOW + 1, false).unwrap();
        assert_eq!(out.relocked, 1);
        assert!(e.store().is_unlocked(10, "A").unwrap());
    }

    #[test]
    fn same_game_two_emulators_keeps_earliest() {
        let tmp = tempfile::tempdir().unwrap();
        onlinefix(tmp.path(), 10, "[A]\nachieved=true\ntimestamp=1700000500\n");
        touch(
            tmp.path(),
            "RoamingAppData/GSE Saves/10/achievements.json",
            r#"{"A":{"earned":true,"earned_time":1700000000}}"#,
        );
        let mut e = engine(tmp.path());
        let out = e.sync(NOW, true).unwrap();
        assert_eq!(out.new_unlocks.len(), 1);
        assert_eq!(e.store().unlock_time(10, "a").unwrap(), Some(1_700_000_000));
    }

    #[test]
    fn notifications_are_ordered_by_time() {
        let tmp = tempfile::tempdir().unwrap();
        let p = onlinefix(tmp.path(), 10, "[A]\nachieved=false\n");
        let mut e = engine(tmp.path());
        e.sync(NOW, true).unwrap();
        std::fs::write(
            &p,
            format!(
                "[A]\nachieved=true\ntimestamp={}\n[B]\nachieved=true\ntimestamp={}\n",
                NOW + 9,
                NOW + 3
            ),
        )
        .unwrap();
        let out = e.sync(NOW + 10, false).unwrap();
        let names: Vec<&str> = out
            .new_unlocks
            .iter()
            .map(|u| u.api_name.as_str())
            .collect();
        assert_eq!(names, ["B", "A"]);
    }

    fn codex(root: &Path, app: u32, body: &str) -> PathBuf {
        touch(
            root,
            &format!("PublicDocuments/Steam/CODEX/{app}/achievements.ini"),
            body,
        )
    }

    #[test]
    fn progress_increase_notifies() {
        let tmp = tempfile::tempdir().unwrap();
        let p = codex(
            tmp.path(),
            50,
            "[A]\nAchieved=0\nCurProgress=1\nMaxProgress=10\n",
        );
        let mut e = engine(tmp.path());
        assert!(e.sync(NOW, true).unwrap().progress.is_empty());
        std::fs::write(&p, "[A]\nAchieved=0\nCurProgress=2\nMaxProgress=10\n").unwrap();
        assert!(e.sync(NOW + 1, false).unwrap().progress.is_empty());
        std::fs::write(&p, "[A]\nAchieved=0\nCurProgress=3\nMaxProgress=10\n").unwrap();
        let out = e.sync(NOW + 2, false).unwrap();
        assert_eq!(
            out.progress,
            vec![ProgressUpdate {
                app_id: 50,
                api_name: "A".into(),
                current: 3,
                max: 10
            }]
        );
    }

    #[test]
    fn progress_regression_is_silent() {
        let tmp = tempfile::tempdir().unwrap();
        let p = codex(
            tmp.path(),
            50,
            "[A]\nAchieved=0\nCurProgress=5\nMaxProgress=10\n",
        );
        let mut e = engine(tmp.path());
        e.sync(NOW, true).unwrap();
        std::fs::write(&p, "[A]\nAchieved=0\nCurProgress=1\nMaxProgress=10\n").unwrap();
        assert!(e.sync(NOW + 1, false).unwrap().progress.is_empty());
        std::fs::write(&p, "[A]\nAchieved=0\nCurProgress=3\nMaxProgress=10\n").unwrap();
        assert_eq!(e.sync(NOW + 2, false).unwrap().progress.len(), 1);
    }

    #[test]
    fn milestones_at_quarter_steps() {
        assert!(crosses_milestone(Some(74), 75, 100));
        assert!(!crosses_milestone(Some(75), 76, 100));
        assert!(crosses_milestone(Some(98), 99, 100));
        assert!(crosses_milestone(None, 30, 100));
        assert!(!crosses_milestone(Some(10), 11, 100));
        assert!(!crosses_milestone(Some(5), 5, 100));
        assert!(!crosses_milestone(Some(0), 1, 1));
        assert!(crosses_milestone(Some(1), 2, 3));
    }

    #[test]
    fn smartsteam_waits_for_schema() {
        let tmp = tempfile::tempdir().unwrap();
        let mut bytes = 1u32.to_le_bytes().to_vec();
        let mut entry = vec![0u8; 24];
        entry[0..4].copy_from_slice(&crc32(b"ACH_WIN").to_le_bytes());
        entry[8..12].copy_from_slice(&1_700_000_000u32.to_le_bytes());
        entry[20] = 1;
        bytes.extend(entry);
        let p = tmp.path().join("RoamingAppData/SmartSteamEmu/40/stats.bin");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, &bytes).unwrap();
        let mut e = engine(tmp.path());
        let out = e.sync(NOW, true).unwrap();
        assert!(out.new_unlocks.is_empty());
        assert_eq!(out.files_failed, 1);
        let row = crate::store::SchemaRow {
            api_name: "ACH_WIN".into(),
            display_name: "Win".into(),
            description: String::new(),
            icon: None,
            icon_gray: None,
            hidden: false,
            percent: None,
        };
        e.store()
            .replace_schema(40, "english", None, &[row], NOW)
            .unwrap();
        let out = e.sync(NOW + 1, false).unwrap();
        assert_eq!(out.new_unlocks.len(), 1);
        assert_eq!(out.new_unlocks[0].api_name, "ACH_WIN");
    }

    #[test]
    fn watch_targets_cover_existing_roots_and_parents_of_missing_ones() {
        let tmp = tempfile::tempdir().unwrap();
        onlinefix(tmp.path(), 10, "[A]\nachieved=false\n");
        std::fs::create_dir_all(tmp.path().join("RoamingAppData")).unwrap();
        let e = engine(tmp.path());
        let targets = e.watch_targets();
        assert!(
            targets
                .recursive
                .contains(&tmp.path().join("PublicDocuments").join("OnlineFix"))
        );
        assert!(targets.flat.contains(&tmp.path().join("RoamingAppData")));
    }
}
