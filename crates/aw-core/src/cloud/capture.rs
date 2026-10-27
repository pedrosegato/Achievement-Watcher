use std::collections::{BTreeMap, HashMap};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::cloud::locate::{Category, SaveRoot, expand};
use crate::cloud::portable::{Placeholders, PortablePath};
use crate::cloud::{at, hex, physical};
use crate::store::GameState;

#[derive(Debug, Clone, PartialEq)]
pub struct CapturedFile {
    pub category: Category,
    pub source: PathBuf,
    pub original: PortablePath,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Capture {
    pub files: Vec<CapturedFile>,
    pub state: Option<GameState>,
    pub digest: String,
    pub total_size: u64,
}

impl Capture {
    pub fn has_save(&self) -> bool {
        self.files.iter().any(|f| f.category == Category::Save)
    }
}

type CacheEntry = (u64, SystemTime, String);

pub const TMP_SUFFIX: &str = ".awtmp";
pub const OLD_SUFFIX: &str = ".awold";

const RACY_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Default)]
pub struct HashCache(Mutex<HashMap<PathBuf, CacheEntry>>);

impl HashCache {
    pub fn sha256(&self, path: &Path) -> io::Result<String> {
        let meta = std::fs::metadata(path).map_err(at(path))?;
        let size = meta.len();
        let modified = meta.modified()?;
        if let Some((cached_size, cached_time, hash)) = self.lock().get(path)
            && *cached_size == size
            && *cached_time == modified
        {
            return Ok(hash.clone());
        }
        let hash = file_sha256(path)?;
        let settled = SystemTime::now()
            .duration_since(modified)
            .is_ok_and(|age| age >= RACY_WINDOW);
        if settled {
            self.lock()
                .insert(path.to_path_buf(), (size, modified, hash.clone()));
        }
        Ok(hash)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, CacheEntry>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn is_restore_leftover(name: &str) -> bool {
    name.ends_with(TMP_SUFFIX) || name.ends_with(OLD_SUFFIX)
}

pub fn file_sha256(path: &Path) -> io::Result<String> {
    let mut file = std::fs::File::open(path).map_err(at(path))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(at(path))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

pub fn capture(
    roots: &[SaveRoot],
    ph: &Placeholders,
    state: Option<GameState>,
    cache: &HashCache,
) -> io::Result<Capture> {
    let mut by_physical: BTreeMap<PathBuf, (PathBuf, Category)> = BTreeMap::new();
    for root in roots {
        for found in expand(root, ph) {
            for file in files_under(&found)? {
                let entry = by_physical
                    .entry(physical(&file))
                    .or_insert_with(|| (file.clone(), root.category));
                if root.category == Category::Save {
                    entry.1 = Category::Save;
                }
            }
        }
    }
    let mut files = Vec::with_capacity(by_physical.len());
    for (source, category) in by_physical.into_values() {
        let original = PortablePath::from_absolute(&source, ph).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is outside every known folder", source.display()),
            )
        })?;
        let size = std::fs::metadata(&source).map_err(at(&source))?.len();
        let sha256 = cache.sha256(&source)?;
        files.push(CapturedFile {
            category,
            source,
            original,
            sha256,
            size,
        });
    }
    files.sort_by(|a, b| (a.category, &a.original).cmp(&(b.category, &b.original)));
    let keys: Vec<(Category, PortablePath, String)> = files
        .iter()
        .map(|f| (f.category, f.original.clone(), f.sha256.clone()))
        .collect();
    Ok(Capture {
        digest: digest(&keys, state.as_ref()),
        total_size: files.iter().map(|f| f.size).sum(),
        files,
        state,
    })
}

fn files_under(path: &Path) -> io::Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut out = Vec::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).map_err(at(&dir))? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() && !is_restore_leftover(&entry.file_name().to_string_lossy()) {
                out.push(entry.path());
            }
        }
    }
    Ok(out)
}

pub fn digest(files: &[(Category, PortablePath, String)], state: Option<&GameState>) -> String {
    let mut lines: Vec<String> = files
        .iter()
        .map(|(category, original, sha)| {
            format!(
                "{}\t{}\t{sha}\n",
                category_name(*category),
                original.as_str()
            )
        })
        .collect();
    lines.sort();
    let mut hasher = Sha256::new();
    for line in &lines {
        hasher.update(line.as_bytes());
    }
    match state {
        Some(state) => {
            hasher.update(b"state\n");
            for unlock in &state.unlocks {
                hasher.update(format!("u\t{}\t{}\n", unlock.api_name, unlock.unlocked_at));
            }
            for item in &state.progress {
                hasher.update(format!(
                    "p\t{}\t{}\t{}\n",
                    item.api_name, item.current, item.max
                ));
            }
        }
        None => hasher.update(b"nostate\n"),
    }
    hex(&hasher.finalize())
}

pub fn category_name(category: Category) -> &'static str {
    match category {
        Category::Save => "save",
        Category::EmulatorFiles => "emulator",
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;
    use crate::discovery::folders::KnownFolders;
    use crate::store::{ProgressState, UnlockState};

    fn ph(root: &Path) -> Placeholders {
        Placeholders {
            folders: Some(KnownFolders::under(root)),
            ..Default::default()
        }
    }

    fn root(category: Category, pattern: &str) -> SaveRoot {
        SaveRoot {
            category,
            pattern: PortablePath::new(pattern).unwrap(),
            single_file: false,
        }
    }

    fn state(playtime: i64) -> GameState {
        GameState {
            unlocks: vec![UnlockState {
                api_name: "A".into(),
                unlocked_at: 10,
                detected_at: 11,
                source_id: "gse".into(),
                playtime_seconds: None,
            }],
            progress: vec![ProgressState {
                api_name: "P".into(),
                current: 1,
                max: 5,
            }],
            playtime_seconds: playtime,
            last_played: None,
        }
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn digest_ignores_playtime() {
        let files = [(
            Category::Save,
            PortablePath::new("<winAppData>/x").unwrap(),
            "h".to_owned(),
        )];
        assert_eq!(
            digest(&files, Some(&state(1))),
            digest(&files, Some(&state(999)))
        );
    }

    #[test]
    fn digest_changes_with_unlocks_and_files() {
        let files = [(
            Category::Save,
            PortablePath::new("<winAppData>/x").unwrap(),
            "h".to_owned(),
        )];
        let other = [(
            Category::Save,
            PortablePath::new("<winAppData>/x").unwrap(),
            "h2".to_owned(),
        )];
        let mut more = state(1);
        more.unlocks[0].api_name = "B".into();
        assert_ne!(
            digest(&files, Some(&state(1))),
            digest(&other, Some(&state(1)))
        );
        assert_ne!(digest(&files, Some(&state(1))), digest(&files, Some(&more)));
        assert_ne!(digest(&files, Some(&state(1))), digest(&files, None));
    }

    #[test]
    fn captures_folders_recursively_with_portable_names() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir
            .path()
            .join("RoamingAppData")
            .join("DarkSoulsIII")
            .join("0110");
        write(&game.join("DS30000.sl2"), "save");
        write(&game.join("sub").join("b.bin"), "bb");
        let cache = HashCache::default();
        let capture = capture(
            &[root(Category::Save, "<winAppData>/DarkSoulsIII/*")],
            &ph(dir.path()),
            None,
            &cache,
        )
        .unwrap();
        let names: Vec<&str> = capture.files.iter().map(|f| f.original.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "<winAppData>/DarkSoulsIII/0110/DS30000.sl2",
                "<winAppData>/DarkSoulsIII/0110/sub/b.bin"
            ]
        );
        assert_eq!(capture.total_size, 6);
        assert!(capture.has_save());
    }

    #[test]
    fn save_wins_over_emulator_files_for_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let emu = dir
            .path()
            .join("RoamingAppData")
            .join("GSE Saves")
            .join("480");
        write(&emu.join("achievements.json"), "{}");
        write(&emu.join("remote").join("save.dat"), "s");
        let capture = capture(
            &[
                root(Category::Save, "<winAppData>/GSE Saves/480/remote"),
                root(Category::EmulatorFiles, "<winAppData>/GSE Saves/480"),
            ],
            &ph(dir.path()),
            Some(state(1)),
            &HashCache::default(),
        )
        .unwrap();
        let got: Vec<(Category, &str)> = capture
            .files
            .iter()
            .map(|f| (f.category, f.original.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                (Category::Save, "<winAppData>/GSE Saves/480/remote/save.dat"),
                (
                    Category::EmulatorFiles,
                    "<winAppData>/GSE Saves/480/achievements.json"
                ),
            ]
        );
    }

    #[test]
    fn empty_capture_has_no_save() {
        let dir = tempfile::tempdir().unwrap();
        let capture = capture(
            &[root(Category::Save, "<winAppData>/Nothing")],
            &ph(dir.path()),
            None,
            &HashCache::default(),
        )
        .unwrap();
        assert!(!capture.has_save());
        assert!(capture.files.is_empty());
    }

    #[test]
    fn hash_cache_skips_unchanged_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        std::fs::write(&path, "aaaa").unwrap();
        let modified = SystemTime::now() - std::time::Duration::from_secs(3600);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        let cache = HashCache::default();
        let first = cache.sha256(&path).unwrap();
        std::fs::write(&path, "bbbb").unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_eq!(cache.sha256(&path).unwrap(), first);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified + std::time::Duration::from_secs(5))
            .unwrap();
        assert_ne!(cache.sha256(&path).unwrap(), first);
    }

    #[test]
    fn missing_file_error_names_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone.sav");
        let error = HashCache::default().sha256(&path).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(error.to_string().contains("gone.sav"), "{error}");
    }

    #[cfg(windows)]
    #[test]
    fn linked_folders_are_captured_once() {
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path().join("RoamingAppData");
        write(
            &app_data.join("RUNE").join("7").join("achievements.ini"),
            "[a]",
        );
        if std::os::windows::fs::symlink_dir(app_data.join("RUNE"), app_data.join("CODEX")).is_err()
        {
            return;
        }
        let roots = [
            root(Category::EmulatorFiles, "<winAppData>/CODEX/7"),
            root(Category::EmulatorFiles, "<winAppData>/RUNE/7"),
        ];
        let got = capture(&roots, &ph(dir.path()), None, &HashCache::default()).unwrap();
        assert_eq!(got.files.len(), 1, "{:?}", got.files);
    }

    #[test]
    fn same_size_rewrite_within_one_clock_tick_is_rehashed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save.sl2");
        std::fs::write(&path, "aaaa").unwrap();
        let tick = std::fs::metadata(&path).unwrap().modified().unwrap();
        let cache = HashCache::default();
        let first = cache.sha256(&path).unwrap();

        std::fs::write(&path, "bbbb").unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(tick)
            .unwrap();

        assert_ne!(cache.sha256(&path).unwrap(), first);
        assert_eq!(cache.sha256(&path).unwrap(), file_sha256(&path).unwrap());
    }
}
