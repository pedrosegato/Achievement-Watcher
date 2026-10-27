use std::path::{Path, PathBuf};

const HELPER_PREFIXES: &[&str] = &[
    "unins",
    "unitycrashhandler",
    "vc_redist",
    "vcredist",
    "dxsetup",
    "dxwebsetup",
];
const HELPER_WORDS: &[&str] = &["setup", "install", "crash", "redist", "updater", "report"];
const MAX_DEPTH: usize = 3;

pub fn is_helper_exe(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    let name = name.to_string_lossy().to_lowercase();
    HELPER_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
        || HELPER_WORDS.iter().any(|word| name.contains(word))
}

/// The most likely game executable: the largest `.exe` up to three folders
/// deep that is not an installer, crash handler or redistributable.
pub fn suggest_exe(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, usize, PathBuf)> = None;
    let mut pending = vec![(dir.to_path_buf(), 0)];
    while let Some((folder, depth)) = pending.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                if depth < MAX_DEPTH {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            let is_exe = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
            if !is_exe || is_helper_exe(&path) {
                continue;
            }
            let length = path.as_os_str().len();
            let better = best.as_ref().is_none_or(|(size, best_length, _)| {
                meta.len() > *size || (meta.len() == *size && length < *best_length)
            });
            if better {
                best = Some((meta.len(), length, path));
            }
        }
    }
    best.map(|(_, _, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe(dir: &Path, rel: &str, kb: usize) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; kb * 1024]).unwrap();
    }

    #[test]
    fn suggests_the_largest_game_exe() {
        let dir = tempfile::tempdir().unwrap();
        exe(dir.path(), "Game.exe", 3);
        exe(dir.path(), "UnityCrashHandler64.exe", 9);
        exe(dir.path(), "unins000.exe", 8);
        exe(dir.path(), "Redist/vc_redist.x64.exe", 7);
        assert_eq!(suggest_exe(dir.path()), Some(dir.path().join("Game.exe")));
    }

    #[test]
    fn looks_into_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        exe(dir.path(), "bin/x64/DarkSoulsIII.exe", 1);
        assert_eq!(
            suggest_exe(dir.path()),
            Some(dir.path().join("bin").join("x64").join("DarkSoulsIII.exe"))
        );
    }

    #[test]
    fn no_exe_means_no_suggestion() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("readme.txt"), "hi").unwrap();
        assert_eq!(suggest_exe(dir.path()), None);
    }

    #[test]
    fn helper_processes_are_not_remembered() {
        for helper in [
            "UnityCrashHandler64.exe",
            "CrashReporter.exe",
            "setup.exe",
            "UbisoftGameLauncherInstaller.exe",
        ] {
            assert!(is_helper_exe(Path::new(helper)), "{helper}");
        }
        for game in ["DarkSoulsIII.exe", "eldenring.exe"] {
            assert!(!is_helper_exe(Path::new(game)), "{game}");
        }
    }
}
