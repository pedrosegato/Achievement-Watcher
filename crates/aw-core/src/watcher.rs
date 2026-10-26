use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};

use crate::discovery::Found;
use crate::discovery::gamediscovery::Unresolved;
use crate::discovery::running::KnownGame;
use crate::engine::{CoreSettings, Engine, SyncOutcome, WatchTargets};
use crate::platform::ExternalId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    Synced(SyncOutcome),
    ScanProgress(u8),
    Error(String),
}

const SCAN_DONE: u8 = 100;
const SCAN_MAX_RUNNING: usize = 99;

pub fn scan_percent(done: usize, total: usize) -> u8 {
    if total == 0 {
        return 0;
    }
    u8::try_from((done * 100 / total).min(SCAN_MAX_RUNNING)).unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchOptions {
    pub debounce: Duration,
    pub max_delay: Duration,
    pub tick: Duration,
}

impl Default for WatchOptions {
    fn default() -> WatchOptions {
        WatchOptions {
            debounce: Duration::from_millis(300),
            max_delay: Duration::from_secs(1),
            tick: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub found: Vec<Found>,
    pub unresolved: Vec<Unresolved>,
    pub watched: Vec<PathBuf>,
    pub last_sync_at: Option<i64>,
    pub known_games: Vec<KnownGame>,
    pub pending: Vec<ExternalId>,
}

enum Control {
    Fs,
    Sync,
    Full,
    Settings(Box<CoreSettings>),
    Stop,
}

pub struct WatcherHandle {
    control: Sender<Control>,
    snapshot: Arc<Mutex<Snapshot>>,
    thread: Option<JoinHandle<()>>,
}

impl WatcherHandle {
    pub fn request_full_sync(&self) {
        let _ = self.control.send(Control::Full);
    }

    pub fn request_sync(&self) {
        let _ = self.control.send(Control::Sync);
    }

    pub fn update_settings(&self, settings: CoreSettings) {
        let _ = self.control.send(Control::Settings(Box::new(settings)));
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let _ = self.control.send(Control::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

pub fn start(engine: Engine, events: Sender<WatchEvent>, options: WatchOptions) -> WatcherHandle {
    let (control, inbox) = channel();
    let snapshot = Arc::new(Mutex::new(Snapshot::default()));
    let fs_control = control.clone();
    let shared = Arc::clone(&snapshot);
    let thread = std::thread::Builder::new()
        .name("aw-watcher".into())
        .spawn(move || {
            let fs = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.is_ok() {
                    let _ = fs_control.send(Control::Fs);
                }
            });
            let fs = match fs {
                Ok(fs) => Some(fs),
                Err(error) => {
                    let _ = events.send(WatchEvent::Error(format!(
                        "file system watcher unavailable: {error}"
                    )));
                    None
                }
            };
            Worker {
                engine,
                events,
                options,
                fs,
                watched: HashMap::new(),
                snapshot: shared,
            }
            .run(&inbox);
        })
        .ok();
    WatcherHandle {
        control,
        snapshot,
        thread,
    }
}

struct Worker {
    engine: Engine,
    events: Sender<WatchEvent>,
    options: WatchOptions,
    fs: Option<RecommendedWatcher>,
    watched: HashMap<PathBuf, RecursiveMode>,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl Worker {
    fn run(mut self, inbox: &Receiver<Control>) {
        self.sync(true, false);
        let mut next_tick = Instant::now() + self.options.tick;
        let mut pending: Option<(Instant, Instant)> = None;
        loop {
            let wake = pending.map_or(next_tick, |(_, deadline)| deadline.min(next_tick));
            match inbox.recv_timeout(wake.saturating_duration_since(Instant::now())) {
                Ok(Control::Fs) => {
                    let now = Instant::now();
                    pending = Some(match pending {
                        Some((first, _)) => (
                            first,
                            (now + self.options.debounce).min(first + self.options.max_delay),
                        ),
                        None => (now, now + self.options.debounce),
                    });
                }
                Ok(Control::Sync) => {
                    pending = None;
                    self.sync(false, false);
                }
                Ok(Control::Full) => {
                    pending = None;
                    self.sync(true, true);
                    next_tick = Instant::now() + self.options.tick;
                }
                Ok(Control::Settings(settings)) => {
                    self.engine.set_settings(*settings);
                    pending = None;
                    self.sync(true, false);
                    next_tick = Instant::now() + self.options.tick;
                }
                Ok(Control::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {
                    let now = Instant::now();
                    if now >= next_tick {
                        pending = None;
                        self.sync(true, false);
                        next_tick = Instant::now() + self.options.tick;
                    } else if pending.is_some_and(|(_, deadline)| now >= deadline) {
                        pending = None;
                        self.sync(false, false);
                    }
                }
            }
        }
    }

    fn sync(&mut self, full: bool, report: bool) {
        let now = unix_now();
        let events = self.events.clone();
        let mut last: Option<u8> = None;
        let mut progress = |done: usize, total: usize| {
            let percent = scan_percent(done, total);
            if report && last != Some(percent) {
                last = Some(percent);
                let _ = events.send(WatchEvent::ScanProgress(percent));
            }
        };
        let event = match self.engine.sync_reporting(now, full, &mut progress) {
            Ok(outcome) => WatchEvent::Synced(outcome),
            Err(error) => WatchEvent::Error(format!("sync failed: {error}")),
        };
        let targets = self.engine.watch_targets();
        self.rearm(&targets);
        {
            let mut snapshot = self.snapshot.lock().unwrap_or_else(PoisonError::into_inner);
            snapshot.found = self.engine.found();
            snapshot.unresolved = self.engine.unresolved().to_vec();
            snapshot.watched = self.watched.keys().cloned().collect();
            snapshot.watched.sort();
            snapshot.last_sync_at = Some(now);
            snapshot.known_games = self.engine.known_games().to_vec();
            snapshot.pending = self.engine.pending().to_vec();
        }
        let _ = self.events.send(event);
        if report {
            let _ = self.events.send(WatchEvent::ScanProgress(SCAN_DONE));
        }
    }

    fn rearm(&mut self, targets: &WatchTargets) {
        let Some(fs) = self.fs.as_mut() else {
            return;
        };
        let desired: HashMap<PathBuf, RecursiveMode> = targets
            .recursive
            .iter()
            .map(|p| (p.clone(), RecursiveMode::Recursive))
            .chain(
                targets
                    .flat
                    .iter()
                    .map(|p| (p.clone(), RecursiveMode::NonRecursive)),
            )
            .collect();
        let stale: Vec<PathBuf> = self
            .watched
            .iter()
            .filter(|(path, mode)| desired.get(*path) != Some(*mode))
            .map(|(path, _)| path.clone())
            .collect();
        for path in stale {
            let _ = fs.unwatch(&path);
            self.watched.remove(&path);
        }
        for (path, mode) in desired {
            if self.watched.contains_key(&path) {
                continue;
            }
            match fs.watch(&path, mode) {
                Ok(()) => {
                    self.watched.insert(path, mode);
                }
                Err(error) => tracing::warn!("cannot watch {}: {error}", path.display()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::folders::KnownFolders;
    use crate::store::Store;
    use std::path::Path;

    fn engine(root: &Path) -> Engine {
        Engine::new(
            Store::in_memory().unwrap(),
            KnownFolders::under(root),
            CoreSettings::default(),
        )
    }

    fn next_unlock_event(rx: &Receiver<WatchEvent>, within: Duration) -> Option<SyncOutcome> {
        let deadline = Instant::now() + within;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(WatchEvent::Synced(outcome)) if !outcome.new_unlocks.is_empty() => {
                    return Some(outcome);
                }
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
        None
    }

    fn wait_first_sync(rx: &Receiver<WatchEvent>) -> SyncOutcome {
        loop {
            match rx.recv_timeout(Duration::from_secs(10)).unwrap() {
                WatchEvent::Synced(outcome) => return outcome,
                WatchEvent::ScanProgress(_) => {}
                WatchEvent::Error(error) => panic!("{error}"),
            }
        }
    }

    fn unlocked_ini(name: &str) -> String {
        format!("[{name}]\nachieved=true\ntimestamp={}\n", unix_now())
    }

    #[test]
    fn scan_percent_holds_below_done_until_the_end() {
        assert_eq!(scan_percent(0, 0), 0);
        assert_eq!(scan_percent(1, 3), 33);
        assert_eq!(scan_percent(3, 3), 99);
    }

    #[test]
    fn unlock_becomes_event_under_one_second() {
        let tmp = tempfile::tempdir().unwrap();
        let stats = tmp.path().join("PublicDocuments/OnlineFix/10/Stats");
        std::fs::create_dir_all(&stats).unwrap();
        let (tx, rx) = channel();
        let handle = start(engine(tmp.path()), tx, WatchOptions::default());
        assert!(wait_first_sync(&rx).first_boot);
        let started = Instant::now();
        std::fs::write(stats.join("Achievements.ini"), unlocked_ini("A")).unwrap();
        let outcome = next_unlock_event(&rx, Duration::from_secs(3)).expect("unlock event");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
        assert!(outcome.new_unlocks[0].notify);
        assert_eq!(outcome.new_unlocks[0].api_name, "A");
        handle.stop();
    }

    #[test]
    fn burst_of_writes_yields_one_event() {
        let tmp = tempfile::tempdir().unwrap();
        let stats = tmp.path().join("PublicDocuments/OnlineFix/10/Stats");
        std::fs::create_dir_all(&stats).unwrap();
        let (tx, rx) = channel();
        let handle = start(engine(tmp.path()), tx, WatchOptions::default());
        wait_first_sync(&rx);
        for _ in 0..10 {
            std::fs::write(stats.join("Achievements.ini"), unlocked_ini("A")).unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        let first = next_unlock_event(&rx, Duration::from_secs(3)).expect("unlock event");
        assert_eq!(first.new_unlocks.len(), 1);
        assert!(next_unlock_event(&rx, Duration::from_millis(1500)).is_none());
        handle.stop();
    }

    #[test]
    fn root_created_later_is_picked_up() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("PublicDocuments")).unwrap();
        let (tx, rx) = channel();
        let handle = start(engine(tmp.path()), tx, WatchOptions::default());
        wait_first_sync(&rx);
        let stats = tmp.path().join("PublicDocuments/OnlineFix/20/Stats");
        std::fs::create_dir_all(&stats).unwrap();
        std::fs::write(stats.join("Achievements.ini"), unlocked_ini("B")).unwrap();
        let outcome = next_unlock_event(&rx, Duration::from_secs(3)).expect("unlock event");
        assert_eq!(outcome.new_unlocks[0].app_id, 20);
        let later = tmp.path().join("PublicDocuments/OnlineFix/21/Stats");
        std::fs::create_dir_all(&later).unwrap();
        std::fs::write(later.join("Achievements.ini"), unlocked_ini("C")).unwrap();
        let outcome = next_unlock_event(&rx, Duration::from_secs(3)).expect("second unlock");
        assert_eq!(outcome.new_unlocks[0].app_id, 21);
        handle.stop();
    }

    #[test]
    fn temp_then_rename_is_seen() {
        let tmp = tempfile::tempdir().unwrap();
        let stats = tmp.path().join("PublicDocuments/OnlineFix/10/Stats");
        std::fs::create_dir_all(&stats).unwrap();
        std::fs::write(stats.join("Achievements.ini"), "[A]\nachieved=false\n").unwrap();
        let (tx, rx) = channel();
        let handle = start(engine(tmp.path()), tx, WatchOptions::default());
        wait_first_sync(&rx);
        let temp = stats.join("Achievements.ini.tmp");
        std::fs::write(&temp, unlocked_ini("A")).unwrap();
        std::fs::rename(&temp, stats.join("Achievements.ini")).unwrap();
        let outcome = next_unlock_event(&rx, Duration::from_secs(3)).expect("unlock event");
        assert_eq!(outcome.new_unlocks[0].api_name, "A");
        handle.stop();
    }

    #[test]
    fn snapshot_lists_found_files_and_watched_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let stats = tmp.path().join("PublicDocuments/OnlineFix/10/Stats");
        std::fs::create_dir_all(&stats).unwrap();
        std::fs::write(stats.join("Achievements.ini"), "[A]\nachieved=false\n").unwrap();
        let (tx, rx) = channel();
        let handle = start(engine(tmp.path()), tx, WatchOptions::default());
        wait_first_sync(&rx);
        let snapshot = handle.snapshot();
        assert_eq!(snapshot.found.len(), 1);
        assert!(
            snapshot
                .watched
                .contains(&tmp.path().join("PublicDocuments").join("OnlineFix"))
        );
        assert!(snapshot.last_sync_at.is_some());
        handle.stop();
    }

    #[test]
    fn request_sync_runs_a_quick_sync() {
        let tmp = tempfile::tempdir().unwrap();
        let (tx, rx) = channel();
        let options = WatchOptions {
            tick: Duration::from_secs(3600),
            ..WatchOptions::default()
        };
        let handle = start(engine(tmp.path()), tx, options);
        wait_first_sync(&rx);
        handle.request_sync();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut synced = false;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(WatchEvent::Synced(_)) => {
                    synced = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(synced);
        handle.stop();
    }
}
