use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::cloud::capture::{Capture, OLD_SUFFIX, TMP_SUFFIX};
use crate::cloud::key::GameKind;
use crate::cloud::locate::{Category, SaveRoot, is_account_segment, wildcard_match};
use crate::cloud::portable::{Placeholders, PortablePath};
use crate::cloud::snapshot::{SnapshotMeta, SnapshotRead, Verify, load_state, read_snapshot};
use crate::cloud::{CancelGate, CloudError, CloudResult, at, copy_file, physical};
use crate::store::Store;

pub struct RestoreTarget<'a> {
    pub app_id: u32,
    pub kind: GameKind,
    pub roots: &'a [SaveRoot],
    pub placeholders: &'a Placeholders,
    pub local: &'a Capture,
    pub base_playtime: Option<i64>,
    /// Where achievements come from when the snapshot has none (it came from
    /// an original copy): the closest cracked snapshot in its history.
    pub inherited_state: Option<&'a Path>,
    pub cancel: Option<&'a CancelGate>,
}

impl RestoreTarget<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.is_some_and(CancelGate::is_cancelled)
    }
}

pub fn restore(
    dir: &Path,
    meta: &SnapshotMeta,
    target: &RestoreTarget,
    store: &Store,
) -> CloudResult<()> {
    if target.cancelled() {
        return Err(CloudError::Cancelled);
    }
    if !matches!(read_snapshot(dir, Verify::Hashes), SnapshotRead::Ready(_)) {
        return Err(CloudError::Incomplete(meta.id.clone()));
    }
    let emulator = target.kind == GameKind::Emulator;
    let applied: Vec<Category> = [
        (Category::Save, meta.contents.save),
        (
            Category::EmulatorFiles,
            meta.contents.emulator_files && emulator,
        ),
    ]
    .into_iter()
    .filter_map(|(category, on)| on.then_some(category))
    .collect();
    let mut writes: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut written: HashSet<String> = HashSet::new();
    let mut covered: HashSet<String> = HashSet::new();
    for entry in meta.files.iter().filter(|e| applied.contains(&e.category)) {
        let Some(original) = local_original(&entry.original, target.app_id, target.roots) else {
            continue;
        };
        if let Some(destination) = retarget(&original, target.roots, target.placeholders)?
            && written.insert(same_file(&destination))
        {
            covered.extend(root_instance(&destination, &original, target.roots));
            writes.push((dir.join(&entry.stored), destination));
        }
    }
    let mut staged: Vec<(PathBuf, &PathBuf)> = Vec::with_capacity(writes.len());
    for (source, destination) in &writes {
        if target.cancelled() {
            discard(&staged);
            return Err(CloudError::Cancelled);
        }
        let temp = suffixed(destination, TMP_SUFFIX);
        let copied = destination
            .parent()
            .map_or(Ok(()), |parent| {
                std::fs::create_dir_all(parent).map_err(at(parent))
            })
            .and_then(|()| copy_file(source, &temp).map(|_| ()));
        if let Err(error) = copied {
            let _ = std::fs::remove_file(&temp);
            discard(&staged);
            return Err(error.into());
        }
        staged.push((temp, destination));
    }
    let swap = match target.cancel {
        Some(gate) => match gate.begin_swap() {
            Some(guard) => Some(guard),
            None => {
                discard(&staged);
                return Err(CloudError::Cancelled);
            }
        },
        None => None,
    };
    let mut swapped: Vec<(&PathBuf, Option<PathBuf>)> = Vec::with_capacity(staged.len());
    for (temp, destination) in &staged {
        match swap_in(temp, destination) {
            Ok(backup) => swapped.push((destination, backup)),
            Err(error) => {
                roll_back(&swapped);
                discard(&staged);
                return Err(error.into());
            }
        }
    }
    let state_dir = if meta.contents.state {
        Some(dir)
    } else {
        target.inherited_state
    };
    if let Some(state_dir) = state_dir.filter(|_| emulator) {
        let applied_state = load_state(state_dir).and_then(|state| {
            let mut state = state.unwrap_or_default();
            if let (Some(base), Some(local)) = (target.base_playtime, target.local.state.as_ref()) {
                let unsynced = local.playtime_seconds.saturating_sub(base).max(0);
                state.playtime_seconds = state.playtime_seconds.max(0).saturating_add(unsynced);
            }
            Ok(store.replace_game_state(target.app_id, &state)?)
        });
        if let Err(error) = applied_state {
            roll_back(&swapped);
            return Err(error);
        }
    }
    drop(swap);
    for backup in swapped.iter().filter_map(|(_, backup)| backup.as_ref()) {
        let _ = remove_forced(backup);
    }
    for file in target
        .local
        .files
        .iter()
        .filter(|f| applied.contains(&f.category))
        .filter(|f| {
            let key = same_file(&f.source);
            covered.iter().any(|root| key.starts_with(root.as_str()))
        })
        .filter(|f| !written.contains(&same_file(&f.source)))
    {
        match std::fs::remove_file(&file.source) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
    }
    Ok(())
}

/// Key under which two paths name the same file: symlinks resolved, and
/// case-folded because Windows file names are case-insensitive.
fn same_file(path: &Path) -> String {
    let key = physical(path).to_string_lossy().into_owned();
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        key
    }
}

/// Moves `temp` over `destination`, keeping the previous file as a backup until
/// the whole restore succeeds.
fn swap_in(temp: &Path, destination: &Path) -> std::io::Result<Option<PathBuf>> {
    let backup = std::fs::symlink_metadata(destination)
        .is_ok()
        .then(|| suffixed(destination, OLD_SUFFIX));
    if let Some(backup) = &backup {
        if std::fs::symlink_metadata(backup).is_ok() {
            remove_forced(backup)?;
        }
        std::fs::rename(destination, backup).map_err(at(destination))?;
    }
    if let Err(error) = std::fs::rename(temp, destination) {
        if let Some(backup) = &backup {
            let _ = std::fs::rename(backup, destination);
        }
        return Err(at(destination)(error));
    }
    Ok(backup)
}

fn roll_back(swapped: &[(&PathBuf, Option<PathBuf>)]) {
    for (destination, backup) in swapped.iter().rev() {
        let _ = std::fs::remove_file(destination);
        if let Some(backup) = backup {
            let _ = std::fs::rename(backup, destination);
        }
    }
}

fn remove_forced(path: &Path) -> std::io::Result<()> {
    if let Ok(meta) = std::fs::metadata(path) {
        let mut permissions = meta.permissions();
        if permissions.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
    std::fs::remove_file(path)
}

/// The concrete folder a root resolved to for this file, e.g. one account's
/// folder under `<storeUserId>`, as a `same_file` key ending in a separator.
fn root_instance(path: &Path, original: &PortablePath, roots: &[SaveRoot]) -> Option<String> {
    let pattern = matching_root(original, roots)?;
    let below = original.as_str().split('/').count() - pattern.len();
    let mut key = same_file(path.ancestors().nth(below)?);
    key.push(std::path::MAIN_SEPARATOR);
    Some(key)
}

fn latest_write(dir: &Path) -> Option<std::time::SystemTime> {
    let mut latest = None;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                pending.push(entry.path());
            } else {
                latest = latest.max(meta.modified().ok());
            }
        }
    }
    latest
}

fn matching_root<'a>(original: &PortablePath, roots: &'a [SaveRoot]) -> Option<Vec<&'a str>> {
    let parts: Vec<&str> = original.as_str().split('/').collect();
    roots.iter().find_map(|root| {
        let pattern: Vec<&str> = root.pattern.as_str().split('/').collect();
        let fits = pattern.len() <= parts.len()
            && pattern[0] == parts[0]
            && pattern
                .iter()
                .zip(&parts)
                .skip(1)
                .all(|(p, o)| wildcard_match(p, o));
        fits.then_some(pattern)
    })
}

pub fn local_original(
    original: &PortablePath,
    app_id: u32,
    roots: &[SaveRoot],
) -> Option<PortablePath> {
    if matching_root(original, roots).is_some() {
        return Some(original.clone());
    }
    let marker = format!("/{app_id}/remote");
    let text = original.as_str();
    let index = text.find(&format!("{marker}/"))?;
    let tail = &text[index + marker.len() + 1..];
    roots
        .iter()
        .filter(|root| root.category == Category::Save)
        .find(|root| root.pattern.as_str().ends_with(&marker))
        .map(|root| root.pattern.join(tail))
}

fn suffixed(destination: &Path, suffix: &str) -> PathBuf {
    let mut name = destination.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn discard(staged: &[(PathBuf, &PathBuf)]) {
    for (temp, _) in staged {
        let _ = std::fs::remove_file(temp);
    }
}

pub fn retarget(
    original: &PortablePath,
    roots: &[SaveRoot],
    ph: &Placeholders,
) -> CloudResult<Option<PathBuf>> {
    let parts: Vec<&str> = original.as_str().split('/').collect();
    let Some(pattern) = matching_root(original, roots) else {
        return Ok(None);
    };
    let Some(mut path) = PortablePath::new(parts[0]).and_then(|token| token.to_absolute(ph)) else {
        return Ok(None);
    };
    for (index, segment) in pattern.iter().enumerate().skip(1) {
        let wanted = parts[index];
        if !is_account_segment(segment) || !is_account_segment(wanted) {
            path.push(wanted);
            continue;
        }
        let mut candidates: Vec<String> = std::fs::read_dir(&path)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        let fixed_tail: Vec<&str> = parts[index + 1..pattern.len()]
            .iter()
            .zip(&pattern[index + 1..])
            .take_while(|(_, segment)| !segment.contains(['<', '*', '?']))
            .map(|(part, _)| *part)
            .collect();
        if candidates.len() > 1 && !fixed_tail.is_empty() {
            let holding: Vec<String> = candidates
                .iter()
                .filter(|name| {
                    fixed_tail
                        .iter()
                        .fold(path.join(name), |p, s| p.join(s))
                        .is_dir()
                })
                .cloned()
                .collect();
            if !holding.is_empty() {
                candidates = holding;
            }
        }
        let chosen = match candidates.as_slice() {
            [] => return Ok(None),
            [only] => only.clone(),
            many => many
                .iter()
                .filter_map(|name| Some((latest_write(&path.join(name))?, name)))
                .max()
                .map(|(_, name)| name.clone())
                .ok_or_else(|| CloudError::MultipleAccounts(original.as_str().to_owned()))?,
        };
        path.push(chosen);
    }
    for rest in &parts[pattern.len()..] {
        path.push(rest);
    }
    if path.is_dir() {
        return Err(CloudError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is a folder", path.display()),
        )));
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::capture::{HashCache, capture};
    use crate::cloud::device::{DeviceId, LocalDevice};
    use crate::cloud::key::GameKind;
    use crate::cloud::layout::GamePaths;
    use crate::cloud::locate::Category;
    use crate::cloud::snapshot::{NewSnapshot, list_snapshots, write_snapshot};
    use crate::discovery::folders::KnownFolders;
    use crate::store::{GameState, UnlockState};

    struct World {
        dir: tempfile::TempDir,
        a: Placeholders,
        b: Placeholders,
        game: GamePaths,
    }

    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let a = Placeholders {
            folders: Some(KnownFolders::under(&dir.path().join("a"))),
            ..Default::default()
        };
        let b = Placeholders {
            folders: Some(KnownFolders::under(&dir.path().join("b"))),
            ..Default::default()
        };
        let game = GamePaths(dir.path().join("cloud").join("games").join("7 - G"));
        World { dir, a, b, game }
    }

    fn pc(w: &World, pc: &str) -> PathBuf {
        w.dir.path().join(pc).join("RoamingAppData")
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn root(category: Category, pattern: &str) -> SaveRoot {
        SaveRoot {
            category,
            pattern: PortablePath::new(pattern).unwrap(),
            single_file: false,
        }
    }

    fn save_roots() -> Vec<SaveRoot> {
        vec![root(Category::Save, "<winAppData>/Game/<storeUserId>")]
    }

    fn emu_roots() -> Vec<SaveRoot> {
        vec![
            root(Category::Save, "<winAppData>/Game/<storeUserId>"),
            root(Category::EmulatorFiles, "<winAppData>/GSE Saves/7"),
        ]
    }

    fn state(unlocks: &[&str], playtime: i64) -> GameState {
        GameState {
            unlocks: unlocks
                .iter()
                .map(|name| UnlockState {
                    api_name: (*name).into(),
                    unlocked_at: 1,
                    detected_at: 1,
                    source_id: "gse".into(),
                    playtime_seconds: None,
                })
                .collect(),
            progress: Vec::new(),
            playtime_seconds: playtime,
            last_played: Some(1),
        }
    }

    fn snapshot(
        w: &World,
        roots: &[SaveRoot],
        state: Option<GameState>,
    ) -> (PathBuf, SnapshotMeta) {
        let cap = capture(roots, &w.a, state, &HashCache::default()).unwrap();
        let device = LocalDevice {
            id: DeviceId("a".into()),
            name: "A".into(),
        };
        let meta = write_snapshot(&NewSnapshot {
            game: &w.game,
            game_key: "7",
            game_name: "G",
            device: &device,
            now: 1,
            session_ended_at: 1,
            parent: None,
            found_on_device: false,
            kept: false,
            local_app_id: 7,
            capture: &cap,
        })
        .unwrap();
        let dir = list_snapshots(std::slice::from_ref(&w.game))
            .into_iter()
            .find(|(_, m)| m.id == meta.id)
            .unwrap()
            .0;
        (dir, meta)
    }

    fn local_of(w: &World, roots: &[SaveRoot], state: Option<GameState>) -> Capture {
        capture(roots, &w.b, state, &HashCache::default()).unwrap()
    }

    fn apply(
        w: &World,
        (dir, meta): &(PathBuf, SnapshotMeta),
        kind: GameKind,
        roots: &[SaveRoot],
        local: &Capture,
        store: &Store,
        base_playtime: Option<i64>,
    ) -> CloudResult<()> {
        restore(
            dir,
            meta,
            &RestoreTarget {
                inherited_state: None,
                cancel: None,
                app_id: 7,
                kind,
                roots,
                placeholders: &w.b,
                local,
                base_playtime,
            },
            store,
        )
    }

    #[test]
    fn restores_files_and_removes_extras_in_applied_categories() {
        let w = world();
        put(&pc(&w, "a").join("Game/111/slot.sav"), "cloud");
        let snap = snapshot(&w, &save_roots(), None);
        put(&pc(&w, "b").join("Game/111/slot.sav"), "local");
        put(&pc(&w, "b").join("Game/111/extra.sav"), "extra");
        let roots = save_roots();
        let local = local_of(&w, &roots, None);
        let store = Store::in_memory().unwrap();
        apply(
            &w,
            &snap,
            GameKind::SteamOriginal,
            &roots,
            &local,
            &store,
            None,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(pc(&w, "b").join("Game/111/slot.sav")).unwrap(),
            "cloud"
        );
        assert!(!pc(&w, "b").join("Game/111/extra.sav").exists());
        assert!(!pc(&w, "b").join("Game/111/slot.sav.awtmp").exists());
    }

    #[test]
    fn save_only_snapshot_never_touches_emulator_files_or_state() {
        let w = world();
        put(&pc(&w, "a").join("Game/111/slot.sav"), "cloud");
        let snap = snapshot(&w, &save_roots(), None);
        put(&pc(&w, "b").join("Game/111/slot.sav"), "local");
        put(&pc(&w, "b").join("GSE Saves/7/achievements.json"), "{}");
        let store = Store::in_memory().unwrap();
        store.record_unlock(7, "A", 1, 1, "gse").unwrap();
        let roots = emu_roots();
        let local = local_of(&w, &roots, Some(store.game_state(7).unwrap()));
        apply(&w, &snap, GameKind::Emulator, &roots, &local, &store, None).unwrap();
        assert!(pc(&w, "b").join("GSE Saves/7/achievements.json").exists());
        assert!(store.is_unlocked(7, "A").unwrap());
        assert_eq!(
            std::fs::read_to_string(pc(&w, "b").join("Game/111/slot.sav")).unwrap(),
            "cloud"
        );
    }

    #[test]
    fn emulator_target_gets_files_and_state() {
        let w = world();
        put(&pc(&w, "a").join("Game/111/slot.sav"), "cloud");
        put(&pc(&w, "a").join("GSE Saves/7/achievements.json"), "[1]");
        let snap = snapshot(&w, &emu_roots(), Some(state(&["A", "B"], 3600)));
        let store = Store::in_memory().unwrap();
        let roots = emu_roots();
        let local = local_of(&w, &roots, Some(store.game_state(7).unwrap()));
        apply(&w, &snap, GameKind::Emulator, &roots, &local, &store, None).unwrap();
        assert_eq!(
            std::fs::read_to_string(pc(&w, "b").join("GSE Saves/7/achievements.json")).unwrap(),
            "[1]"
        );
        assert!(store.is_unlocked(7, "B").unwrap());
        assert_eq!(store.aw_playtime_seconds(7).unwrap(), 3600);
    }

    #[test]
    fn steam_original_target_ignores_state_and_emulator_files() {
        let w = world();
        put(&pc(&w, "a").join("Game/111/slot.sav"), "cloud");
        put(&pc(&w, "a").join("GSE Saves/7/achievements.json"), "[1]");
        let snap = snapshot(&w, &emu_roots(), Some(state(&["A"], 3600)));
        let store = Store::in_memory().unwrap();
        let roots = save_roots();
        let local = local_of(&w, &roots, None);
        apply(
            &w,
            &snap,
            GameKind::SteamOriginal,
            &roots,
            &local,
            &store,
            None,
        )
        .unwrap();
        assert!(!pc(&w, "b").join("GSE Saves/7/achievements.json").exists());
        assert!(!store.is_unlocked(7, "A").unwrap());
        assert!(pc(&w, "b").join("Game/111/slot.sav").exists());
    }

    #[test]
    fn another_accounts_folder_is_left_alone() {
        let w = world();
        put(&pc(&w, "a").join("Game/AAAA/DS30000.sl2"), "cloud");
        let snap = snapshot(&w, &save_roots(), None);
        std::fs::create_dir_all(pc(&w, "b").join("Game/BBBB")).unwrap();
        let roots = save_roots();
        let local = local_of(&w, &roots, None);
        let store = Store::in_memory().unwrap();
        apply(
            &w,
            &snap,
            GameKind::SteamOriginal,
            &roots,
            &local,
            &store,
            None,
        )
        .unwrap();
        assert!(pc(&w, "b").join("Game/AAAA/DS30000.sl2").exists());
        assert!(!pc(&w, "b").join("Game/BBBB/DS30000.sl2").exists());
    }

    #[test]
    fn no_local_account_folder_keeps_original_id() {
        let w = world();
        put(&pc(&w, "a").join("Game/AAAA/DS30000.sl2"), "cloud");
        let snap = snapshot(&w, &save_roots(), None);
        let roots = save_roots();
        let local = local_of(&w, &roots, None);
        let store = Store::in_memory().unwrap();
        apply(
            &w,
            &snap,
            GameKind::SteamOriginal,
            &roots,
            &local,
            &store,
            None,
        )
        .unwrap();
        assert!(pc(&w, "b").join("Game/AAAA/DS30000.sl2").exists());
    }

    #[test]
    fn several_accounts_never_make_the_restore_guess() {
        let w = world();
        put(&pc(&w, "a").join("Game/AAAA/DS30000.sl2"), "cloud");
        let snap = snapshot(&w, &save_roots(), None);
        std::fs::create_dir_all(pc(&w, "b").join("Game/AAAA")).unwrap();
        std::fs::create_dir_all(pc(&w, "b").join("Game/CCCC")).unwrap();
        let roots = save_roots();
        let store = Store::in_memory().unwrap();
        let local = local_of(&w, &roots, None);
        apply(
            &w,
            &snap,
            GameKind::SteamOriginal,
            &roots,
            &local,
            &store,
            None,
        )
        .unwrap();
        assert!(pc(&w, "b").join("Game/AAAA/DS30000.sl2").exists());

        std::fs::remove_dir_all(pc(&w, "b").join("Game/AAAA")).unwrap();
        std::fs::create_dir_all(pc(&w, "b").join("Game/DDDD")).unwrap();
        let local = local_of(&w, &roots, None);
        apply(
            &w,
            &snap,
            GameKind::SteamOriginal,
            &roots,
            &local,
            &store,
            None,
        )
        .unwrap();
        assert!(pc(&w, "b").join("Game/AAAA/DS30000.sl2").exists());
        assert!(!pc(&w, "b").join("Game/CCCC/DS30000.sl2").exists());
        assert!(!pc(&w, "b").join("Game/DDDD/DS30000.sl2").exists());
    }

    #[test]
    fn unsnapshotted_hours_survive_a_restore() {
        let w = world();
        put(&pc(&w, "a").join("Game/111/slot.sav"), "cloud");
        let snap = snapshot(&w, &save_roots(), Some(state(&[], 3600)));
        let store = Store::in_memory().unwrap();
        store.add_playtime(7, 1300, 1).unwrap();
        let roots = save_roots();
        let local = local_of(&w, &roots, Some(store.game_state(7).unwrap()));
        apply(
            &w,
            &snap,
            GameKind::Emulator,
            &roots,
            &local,
            &store,
            Some(1000),
        )
        .unwrap();
        assert_eq!(store.aw_playtime_seconds(7).unwrap(), 3900);
    }

    #[test]
    fn tampered_snapshot_is_not_applied() {
        let w = world();
        put(&pc(&w, "a").join("Game/111/slot.sav"), "cloud");
        let snap = snapshot(&w, &save_roots(), None);
        std::fs::write(snap.0.join(&snap.1.files[0].stored), "CLOUD").unwrap();
        put(&pc(&w, "b").join("Game/111/slot.sav"), "local");
        let roots = save_roots();
        let local = local_of(&w, &roots, None);
        let store = Store::in_memory().unwrap();
        let result = apply(
            &w,
            &snap,
            GameKind::SteamOriginal,
            &roots,
            &local,
            &store,
            None,
        );
        assert!(matches!(result, Err(CloudError::Incomplete(_))));
        assert_eq!(
            std::fs::read_to_string(pc(&w, "b").join("Game/111/slot.sav")).unwrap(),
            "local"
        );
    }

    #[test]
    fn file_glob_segment_keeps_the_original_name() {
        let w = world();
        std::fs::create_dir_all(pc(&w, "b").join("Game")).unwrap();
        std::fs::write(pc(&w, "b").join("Game").join("save9.dat"), "x").unwrap();
        let roots = vec![root(Category::Save, "<winAppData>/Game/save*.dat")];
        let original = PortablePath::new("<winAppData>/Game/save1.dat").unwrap();
        assert_eq!(
            retarget(&original, &roots, &w.b).unwrap(),
            Some(pc(&w, "b").join("Game").join("save1.dat"))
        );
    }

    #[test]
    fn destination_that_is_a_folder_is_refused() {
        let w = world();
        std::fs::create_dir_all(pc(&w, "b").join("Game").join("slot.sav")).unwrap();
        let roots = vec![root(Category::Save, "<winAppData>/Game")];
        let original = PortablePath::new("<winAppData>/Game/slot.sav").unwrap();
        assert!(retarget(&original, &roots, &w.b).is_err());
    }

    #[test]
    fn staging_failure_changes_nothing() {
        let w = world();
        put(&pc(&w, "a").join("Game/111/a.sav"), "cloud a");
        put(&pc(&w, "a").join("Game/111/sub/b.sav"), "cloud b");
        let snap = snapshot(&w, &save_roots(), Some(state(&["CLOUD"], 10)));
        put(&pc(&w, "b").join("Game/111/a.sav"), "local a");
        put(&pc(&w, "b").join("Game/111/sub"), "i am a file");
        let store = Store::in_memory().unwrap();
        store.record_unlock(7, "LOCAL", 1, 1, "gse").unwrap();
        let roots = save_roots();
        let local = local_of(&w, &roots, Some(store.game_state(7).unwrap()));
        let result = apply(&w, &snap, GameKind::Emulator, &roots, &local, &store, None);
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(pc(&w, "b").join("Game/111/a.sav")).unwrap(),
            "local a"
        );
        assert!(store.is_unlocked(7, "LOCAL").unwrap());
        assert!(!store.is_unlocked(7, "CLOUD").unwrap());
        let leftovers: Vec<_> = std::fs::read_dir(pc(&w, "b").join("Game/111"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".awtmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn linked_destinations_are_written_once() {
        let w = world();
        put(&pc(&w, "a").join("CODEX/7/achievements.ini"), "[cloud]");
        put(&pc(&w, "a").join("RUNE/7/achievements.ini"), "[cloud]");
        let roots = vec![
            root(Category::EmulatorFiles, "<winAppData>/CODEX/7"),
            root(Category::EmulatorFiles, "<winAppData>/RUNE/7"),
        ];
        let snap = snapshot(&w, &roots, Some(state(&[], 0)));
        put(&pc(&w, "b").join("RUNE/7/achievements.ini"), "[local]");
        if std::os::windows::fs::symlink_dir(pc(&w, "b").join("RUNE"), pc(&w, "b").join("CODEX"))
            .is_err()
        {
            return;
        }
        let store = Store::in_memory().unwrap();
        let local = local_of(&w, &roots, Some(store.game_state(7).unwrap()));
        apply(&w, &snap, GameKind::Emulator, &roots, &local, &store, None).unwrap();
        assert_eq!(
            std::fs::read_to_string(pc(&w, "b").join("RUNE/7/achievements.ini")).unwrap(),
            "[cloud]"
        );
        let leftovers: Vec<_> = std::fs::read_dir(pc(&w, "b").join("RUNE/7"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".awtmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn cancelled_restore_changes_nothing_and_leaves_no_temp_files() {
        let w = world();
        put(&pc(&w, "a").join("Game/111/a.sav"), "cloud a");
        put(&pc(&w, "a").join("Game/111/b.sav"), "cloud b");
        let snap = snapshot(&w, &emu_roots(), Some(state(&["CLOUD"], 10)));
        put(&pc(&w, "b").join("Game/111/a.sav"), "local a");
        put(&pc(&w, "b").join("Game/111/b.sav"), "local b");
        let store = Store::in_memory().unwrap();
        store.record_unlock(7, "LOCAL", 1, 1, "gse").unwrap();
        let before = store.game_state(7).unwrap();
        let roots = emu_roots();
        let local = local_of(&w, &roots, Some(before.clone()));
        let cancel = CancelGate::default();
        cancel.cancel();
        let result = restore(
            &snap.0,
            &snap.1,
            &RestoreTarget {
                inherited_state: None,
                app_id: 7,
                kind: GameKind::Emulator,
                roots: &roots,
                placeholders: &w.b,
                local: &local,
                base_playtime: None,
                cancel: Some(&cancel),
            },
            &store,
        );
        assert!(matches!(result, Err(CloudError::Cancelled)), "{result:?}");
        let dir = pc(&w, "b").join("Game/111");
        assert_eq!(
            std::fs::read_to_string(dir.join("a.sav")).unwrap(),
            "local a"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("b.sav")).unwrap(),
            "local b"
        );
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(TMP_SUFFIX) || n.ends_with(OLD_SUFFIX))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        assert_eq!(store.game_state(7).unwrap(), before);
    }
}
