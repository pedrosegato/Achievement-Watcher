use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;

use crate::discovery::detect::{DetectConfig, detect_in_game_folders, merge_found};
use crate::discovery::folders::{Base, KnownFolders};
use crate::discovery::gamediscovery::Unresolved;
use crate::discovery::lumaplay::{self, Registry, WindowsRegistry};
use crate::discovery::running::KnownGame;
use crate::discovery::sources::builtin_sources;
use crate::discovery::{Found, discover};
use crate::fileio::{fnv1a64, read_with_retry};
use crate::model::Unlock;
use crate::model::sane_unlock_time;
use crate::parsers::smartsteam::{CRC_PREFIX, crc_name, crc32};
use crate::parsers::{local_texts, parse_file};
use crate::platform::{ExternalId, Platform, is_synthetic, map_name, uplay_key};
use crate::store::{Store, StoreResult};

const FIRST_BOOT_KEY: &str = "first_boot_done";
const NAMES_PENDING: &str = "achievement names not available yet";
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
    pub pending: Vec<ExternalId>,
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
    registry: Arc<dyn Registry>,
    identities: HashMap<ExternalId, u32>,
    pending: Vec<ExternalId>,
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
            registry: Arc::new(WindowsRegistry),
            identities: HashMap::new(),
            pending: Vec::new(),
        }
    }

    pub fn set_registry(&mut self, registry: Arc<dyn Registry>) {
        self.registry = registry;
    }

    pub fn pending(&self) -> &[ExternalId] {
        &self.pending
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
            .into_iter()
            .map(|mut file| {
                if let Some(app_id) = file
                    .external
                    .as_ref()
                    .and_then(|key| self.identities.get(key))
                {
                    file.app_id = *app_id;
                }
                file
            })
            .collect()
    }

    fn resolve_identities(&mut self) -> StoreResult<Vec<Found>> {
        self.identities.clear();
        let mut pending = BTreeSet::new();
        let mut found = Vec::new();
        for mut file in merge_found(self.fixed_found.clone(), self.game_found.clone()) {
            if let Some(key) = &file.external {
                match self.store.external_app_id(key)? {
                    Some(app_id) => {
                        self.identities.insert(key.clone(), app_id);
                        file.app_id = app_id;
                    }
                    None => {
                        pending.insert(key.clone());
                        continue;
                    }
                }
            }
            found.push(file);
        }
        found.sort_by(|a, b| {
            (a.app_id, a.source_id, &a.path).cmp(&(b.app_id, b.source_id, &b.path))
        });
        self.pending = pending.into_iter().collect();
        Ok(found)
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
        self.fixed_found
            .extend(lumaplay::discover(self.registry.as_ref()));
        if full {
            let in_games = detect_in_game_folders(&self.folders, &self.settings.detect);
            self.game_found = in_games.found;
            self.unresolved = in_games.unresolved;
            self.known_games = in_games.games;
        }
        let found = self.resolve_identities()?;
        let first_boot = self.store.meta_get(FIRST_BOOT_KEY)?.is_none();
        let mut outcome = SyncOutcome {
            first_boot,
            games_seen: found
                .iter()
                .map(|f| f.app_id)
                .collect::<BTreeSet<_>>()
                .len(),
            pending: self.pending.clone(),
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
        let bytes = match self.read_bytes(file) {
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
        let previous = self.store.file_hash(&path)?;
        if previous.as_deref() == Some(hash.as_str()) {
            return Ok(());
        }
        let untimed = previous
            .is_none()
            .then(|| self.written_at(file))
            .flatten()
            .and_then(|written| sane_unlock_time(written, now))
            .unwrap_or(now);
        let parsed = parse_file(file.format, &file.path, &bytes)
            .map_err(|error| error.to_string())
            .and_then(|unlocks| self.resolve_names(file, unlocks));
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
        if file.external.is_some() && is_synthetic(file.app_id) {
            let texts = local_texts(file.format, &bytes);
            if !texts.is_empty() {
                self.store.replace_local_schema(file.app_id, &texts, now)?;
            }
        }
        for unlock in &unlocks {
            if unlock.unlocked {
                let time = unlock
                    .unlocked_at
                    .and_then(|raw| sane_unlock_time(raw, now))
                    .unwrap_or(untimed);
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
    fn written_at(&self, file: &Found) -> Option<i64> {
        if lumaplay::is_registry_path(&file.path) {
            return lumaplay::written_at(self.registry.as_ref(), &file.path);
        }
        let modified = std::fs::metadata(&file.path).ok()?.modified().ok()?;
        let seconds = modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();
        i64::try_from(seconds).ok()
    }

    fn read_bytes(&self, file: &Found) -> std::io::Result<Vec<u8>> {
        if lumaplay::is_registry_path(&file.path) {
            lumaplay::snapshot(self.registry.as_ref(), &file.path)
        } else {
            read_with_retry(&file.path, READ_BUDGET)
        }
    }

    fn resolve_names(&self, file: &Found, unlocks: Vec<Unlock>) -> Result<Vec<Unlock>, String> {
        let unlocks = self.resolve_crc_names(file, unlocks)?;
        let Some(external) = &file.external else {
            return Ok(unlocks);
        };
        let platform = external.platform;
        if is_synthetic(file.app_id) {
            return Ok(rename(unlocks, |raw| fallback_name(platform, raw)));
        }
        if self
            .store
            .schema_info(file.app_id)
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Err(NAMES_PENDING.to_owned());
        }
        let schema = self
            .store
            .schema_names(file.app_id)
            .map_err(|e| e.to_string())?;
        Ok(rename(unlocks, |raw| {
            map_name(platform, raw, &schema).unwrap_or_else(|| fallback_name(platform, raw))
        }))
    }

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
                        .ok_or_else(|| NAMES_PENDING.to_owned())?;
                }
                Ok(unlock)
            })
            .collect()
    }
}

fn rename(unlocks: Vec<Unlock>, name: impl Fn(&str) -> String) -> Vec<Unlock> {
    unlocks
        .into_iter()
        .map(|mut unlock| {
            unlock.api_name = name(&unlock.api_name);
            unlock
        })
        .collect()
}

fn fallback_name(platform: Platform, raw: &str) -> String {
    match platform {
        Platform::Ubisoft => uplay_key(raw),
        _ => raw.to_owned(),
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
    use crate::discovery::lumaplay::MemoryRegistry;
    use crate::platform::{ExternalId, GameImages, Platform};
    use std::sync::Arc;

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
    fn restored_unlocks_do_not_notify() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e = engine(tmp.path());
        e.sync(NOW, true).unwrap();
        let state = crate::store::GameState {
            unlocks: vec![crate::store::UnlockState {
                api_name: "A".into(),
                unlocked_at: NOW - 30,
                detected_at: NOW - 30,
                source_id: "gse".into(),
                playtime_seconds: None,
            }],
            ..Default::default()
        };
        e.store().replace_game_state(10, &state).unwrap();
        touch(
            tmp.path(),
            "RoamingAppData/GSE Saves/10/achievements.json",
            &format!(r#"{{"A":{{"earned":true,"earned_time":{}}}}}"#, NOW - 30),
        );
        let out = e.sync(NOW + 1, true).unwrap();
        assert!(out.new_unlocks.is_empty(), "{:?}", out.new_unlocks);
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

    fn lumaplay(id: &str, values: &[(&str, u64)]) -> Arc<dyn Registry> {
        let mut r = MemoryRegistry::default();
        r.keys
            .insert(r"SOFTWARE\LumaPlay".into(), vec!["player".into()]);
        r.keys
            .insert(r"SOFTWARE\LumaPlay\player".into(), vec![id.into()]);
        r.values.insert(
            format!(r"SOFTWARE\LumaPlay\player\{id}\Achievements"),
            values.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect(),
        );
        Arc::new(r)
    }

    fn schema(e: &Engine, app_id: u32, names: &[&str]) {
        let rows: Vec<crate::store::SchemaRow> = names
            .iter()
            .map(|name| crate::store::SchemaRow {
                api_name: (*name).into(),
                display_name: (*name).into(),
                description: String::new(),
                icon: None,
                icon_gray: None,
                hidden: false,
                percent: None,
            })
            .collect();
        e.store()
            .replace_schema(app_id, "english", None, &rows, NOW)
            .unwrap();
    }

    fn rune(root: &Path, id: &str, body: &str) -> PathBuf {
        touch(
            root,
            &format!("Documents/RUNE/Ubisoft Connect/achievements/player/{id}/achievements.cfg"),
            body,
        )
    }

    fn ubisoft(id: &str) -> ExternalId {
        ExternalId {
            platform: Platform::Ubisoft,
            id: id.into(),
        }
    }

    const RUNE_ONE: &str = r#""achievements" { "1" { "earned" "1" "time" "1700000000" } }"#;

    #[test]
    fn external_file_waits_for_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let path = touch(
            tmp.path(),
            "RoamingAppData/NemirtingasEpicEmu/player/X/achievements.json",
            r#"{"A":{"unlock_time":1700000000}}"#,
        );
        let mut e = engine(tmp.path());
        let out = e.sync(NOW, true).unwrap();
        assert!(out.new_unlocks.is_empty());
        assert_eq!(
            out.pending,
            vec![ExternalId {
                platform: Platform::Epic,
                id: "X".into()
            }]
        );
        assert_eq!(out.games_seen, 0);
        assert_eq!(e.store().file_hash(&path.to_string_lossy()).unwrap(), None);
        assert!(e.store().games_overview().unwrap().is_empty());
    }

    #[test]
    fn resolved_external_file_is_read_under_steam_id_with_schema_names() {
        let tmp = tempfile::tempdir().unwrap();
        rune(tmp.path(), "4", RUNE_ONE);
        let mut e = engine(tmp.path());
        e.store()
            .save_external_game(
                &ubisoft("4"),
                Some(33230),
                Some("AC II"),
                &GameImages::default(),
                NOW,
            )
            .unwrap();
        schema(&e, 33230, &["ACH_ONE_1", "ACH_TWO_2"]);
        let out = e.sync(NOW, true).unwrap();
        assert!(out.pending.is_empty());
        let got: Vec<(u32, &str)> = out
            .new_unlocks
            .iter()
            .map(|u| (u.app_id, u.api_name.as_str()))
            .collect();
        assert_eq!(got, vec![(33230, "ACH_ONE_1")]);
        assert!(e.found().iter().all(|f| f.app_id == 33230));
    }

    #[test]
    fn resolved_steam_game_without_schema_waits() {
        let tmp = tempfile::tempdir().unwrap();
        rune(tmp.path(), "4", RUNE_ONE);
        let mut e = engine(tmp.path());
        e.store()
            .save_external_game(
                &ubisoft("4"),
                Some(33230),
                None,
                &GameImages::default(),
                NOW,
            )
            .unwrap();
        let out = e.sync(NOW, true).unwrap();
        assert!(out.new_unlocks.is_empty());
        assert_eq!(out.files_failed, 1);
        schema(&e, 33230, &["ACH_ONE_1"]);
        let out = e.sync(NOW + 1, false).unwrap();
        assert_eq!(out.new_unlocks.len(), 1);
        assert_eq!(out.new_unlocks[0].api_name, "ACH_ONE_1");
    }

    #[test]
    fn unmatched_ubisoft_names_fall_back_to_numeric_key() {
        let tmp = tempfile::tempdir().unwrap();
        rune(tmp.path(), "4", RUNE_ONE);
        let mut e = engine(tmp.path());
        e.set_registry(lumaplay("4", &[("ACH_1", 1)]));
        e.store()
            .save_external_game(
                &ubisoft("4"),
                Some(33230),
                None,
                &GameImages::default(),
                NOW,
            )
            .unwrap();
        schema(&e, 33230, &["SOMETHING_ELSE"]);
        let out = e.sync(NOW, true).unwrap();
        let names: Vec<&str> = out
            .new_unlocks
            .iter()
            .map(|u| u.api_name.as_str())
            .collect();
        assert_eq!(names, vec!["1"]);
    }

    #[test]
    fn rune_and_lumaplay_merge_into_one_achievement() {
        let tmp = tempfile::tempdir().unwrap();
        rune(tmp.path(), "4", RUNE_ONE);
        let mut e = engine(tmp.path());
        e.set_registry(lumaplay("4", &[("ACH_1", 1), ("ACH_2", 0)]));
        e.store()
            .save_external_game(
                &ubisoft("4"),
                Some(33230),
                None,
                &GameImages::default(),
                NOW,
            )
            .unwrap();
        schema(&e, 33230, &["ACH_ONE_1", "ACH_TWO_2"]);
        let out = e.sync(NOW, true).unwrap();
        assert_eq!(out.new_unlocks.len(), 1);
        assert_eq!(
            e.store().unlock_time(33230, "ACH_ONE_1").unwrap(),
            Some(1_700_000_000)
        );
        assert!(!e.store().is_unlocked(33230, "ACH_TWO_2").unwrap());
    }

    #[test]
    fn pending_file_is_read_after_resolution_without_change_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        rune(tmp.path(), "4", RUNE_ONE);
        let mut e = engine(tmp.path());
        let out = e.sync(NOW, true).unwrap();
        assert_eq!(out.pending, vec![ubisoft("4")]);
        e.store()
            .save_external_game(
                &ubisoft("4"),
                None,
                Some("Lost game"),
                &GameImages::default(),
                NOW,
            )
            .unwrap();
        let out = e.sync(NOW + 1, false).unwrap();
        assert!(out.pending.is_empty());
        assert_eq!(out.new_unlocks.len(), 1);
        assert!(crate::platform::is_synthetic(out.new_unlocks[0].app_id));
        assert_eq!(out.new_unlocks[0].api_name, "1");
    }

    #[test]
    fn synthetic_nemirtingas_writes_local_schema_once() {
        let tmp = tempfile::tempdir().unwrap();
        let path = touch(
            tmp.path(),
            "RoamingAppData/NemirtingasEpicEmu/player/X/achievements.json",
            r#"{"A":{"display_name":"Alpha","description":"First","unlock_time":1700000000},"B":{"display_name":"Beta","unlock_time":-1}}"#,
        );
        let mut e = engine(tmp.path());
        let key = ExternalId {
            platform: Platform::Epic,
            id: "X".into(),
        };
        let app_id = e
            .store()
            .save_external_game(&key, None, Some("X"), &GameImages::default(), NOW)
            .unwrap();
        e.sync(NOW, true).unwrap();
        assert_eq!(
            e.store().schema_info(app_id).unwrap(),
            Some(("local".into(), NOW))
        );
        assert_eq!(
            e.store()
                .schema_row(app_id, "A")
                .unwrap()
                .unwrap()
                .display_name,
            "Alpha"
        );
        std::fs::write(
            &path,
            r#"{"A":{"display_name":"Alpha","description":"First","unlock_time":1700000000},"B":{"display_name":"Beta","unlock_time":1700000100}}"#,
        )
        .unwrap();
        e.sync(NOW + 5, false).unwrap();
        assert_eq!(
            e.store().schema_info(app_id).unwrap(),
            Some(("local".into(), NOW))
        );
    }

    #[test]
    fn local_schema_never_written_for_steam_mapped_games() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            tmp.path(),
            "RoamingAppData/NemirtingasEpicEmu/player/X/achievements.json",
            r#"{"A":{"display_name":"Alpha","unlock_time":1700000000}}"#,
        );
        let mut e = engine(tmp.path());
        let key = ExternalId {
            platform: Platform::Epic,
            id: "X".into(),
        };
        e.store()
            .save_external_game(&key, Some(10), None, &GameImages::default(), NOW)
            .unwrap();
        e.sync(NOW, true).unwrap();
        assert_eq!(e.store().schema_info(10).unwrap(), None);
    }

    #[test]
    fn lumaplay_found_reads_from_registry() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e = engine(tmp.path());
        e.set_registry(lumaplay("635", &[("ACH_3", 1)]));
        e.store()
            .save_external_game(&ubisoft("635"), None, None, &GameImages::default(), NOW)
            .unwrap();
        let out = e.sync(NOW, true).unwrap();
        assert_eq!(out.new_unlocks.len(), 1);
        assert_eq!(out.new_unlocks[0].api_name, "3");
        assert_eq!(out.new_unlocks[0].unlocked_at, NOW);
        assert_eq!(out.new_unlocks[0].source_id, "lumaplay");
    }

    fn set_mtime(path: &Path, unix: i64) {
        let when = std::time::UNIX_EPOCH + Duration::from_secs(u64::try_from(unix).unwrap());
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }

    fn nemirtingas_after_first_boot(root: &Path, mtime: i64) -> (Engine, u32) {
        let mut e = engine(root);
        e.set_registry(Arc::new(MemoryRegistry::default()));
        e.sync(NOW - 7200, true).unwrap();
        let path = touch(
            root,
            "RoamingAppData/NemirtingasEpicEmu/player/X/achievements.json",
            r#"{"A":{"unlock_time":3600000000000},"B":{"unlock_time":-1}}"#,
        );
        set_mtime(&path, mtime);
        let key = ExternalId {
            platform: Platform::Epic,
            id: "X".into(),
        };
        let app_id = e
            .store()
            .save_external_game(&key, None, None, &GameImages::default(), NOW)
            .unwrap();
        (e, app_id)
    }

    #[test]
    fn first_read_untimed_unlock_uses_file_time_and_stays_silent() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut e, app_id) = nemirtingas_after_first_boot(tmp.path(), NOW - 3600);
        let out = e.sync(NOW, false).unwrap();
        assert_eq!(out.new_unlocks.len(), 1);
        assert_eq!(out.new_unlocks[0].unlocked_at, NOW - 3600);
        assert!(!out.new_unlocks[0].notify);
        assert_eq!(
            e.store().unlock_time(app_id, "A").unwrap(),
            Some(NOW - 3600)
        );
    }

    #[test]
    fn first_read_untimed_unlock_of_fresh_file_notifies() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut e, _) = nemirtingas_after_first_boot(tmp.path(), NOW - 2);
        let out = e.sync(NOW, false).unwrap();
        assert_eq!(out.new_unlocks[0].unlocked_at, NOW - 2);
        assert!(out.new_unlocks[0].notify);
    }

    #[test]
    fn first_read_lumaplay_uses_key_write_time() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e = engine(tmp.path());
        e.set_registry(Arc::new(MemoryRegistry::default()));
        e.sync(NOW - 7200, true).unwrap();
        let mut registry = MemoryRegistry::default();
        registry
            .keys
            .insert(r"SOFTWARE\LumaPlay".into(), vec!["player".into()]);
        registry
            .keys
            .insert(r"SOFTWARE\LumaPlay\player".into(), vec!["9".into()]);
        let key = r"SOFTWARE\LumaPlay\player\9\Achievements".to_owned();
        registry
            .values
            .insert(key.clone(), vec![("ACH_1".into(), 1)]);
        registry.written.insert(key, NOW - 86_400);
        e.set_registry(Arc::new(registry));
        e.store()
            .save_external_game(&ubisoft("9"), None, None, &GameImages::default(), NOW)
            .unwrap();
        let out = e.sync(NOW, false).unwrap();
        assert_eq!(out.new_unlocks[0].unlocked_at, NOW - 86_400);
        assert!(!out.new_unlocks[0].notify);
    }
}
