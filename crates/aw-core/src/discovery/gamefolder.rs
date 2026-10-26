use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::discovery::gamedirs::subdirs;

pub const MAX_DEPTH: usize = 8;
pub const MAX_VISITED: usize = 2000;

const BINARY_DIRS: [&str; 10] = [
    "bin", "bin32", "bin64", "binaries", "win32", "win64", "x64", "x86", "game", "runtime",
];
const UNREAL_PLATFORMS: [&str; 3] = ["Win64", "Win32", "WinGDK"];
const UNITY_PLUGINS: [&str; 2] = ["x86_64", "x86"];
const IGNORED_DIRS: [&str; 20] = [
    ".git",
    "audio",
    "config",
    "configs",
    "content",
    "data",
    "intermediate",
    "localization",
    "logs",
    "movies",
    "music",
    "pak",
    "paks",
    "saved",
    "saves",
    "shaders",
    "sound",
    "sounds",
    "textures",
    "videos",
];
const STEAM_SETTINGS: &str = "steam_settings";

pub fn base_dirs(root: &Path) -> Vec<PathBuf> {
    let children = subdirs(root);
    let mut candidates = vec![root.to_path_buf()];
    candidates.extend(
        children
            .iter()
            .filter(|dir| name_in(dir, &BINARY_DIRS))
            .cloned(),
    );
    for child in &children {
        let binaries = child.join("Binaries");
        candidates.extend(UNREAL_PLATFORMS.iter().map(|p| binaries.join(p)));
    }
    for child in children
        .iter()
        .filter(|dir| name_lower(dir).ends_with("_data"))
    {
        let plugins = child.join("Plugins");
        candidates.extend(UNITY_PLUGINS.iter().map(|p| plugins.join(p)));
    }
    let steamworks = root
        .join("Engine")
        .join("Binaries")
        .join("ThirdParty")
        .join("Steamworks");
    for version in subdirs(&steamworks) {
        candidates.extend(UNREAL_PLATFORMS.iter().map(|p| version.join(p)));
    }
    contained_unique(root, candidates)
}

pub fn steam_settings_dirs(root: &Path, bases: &[PathBuf], deep_search: bool) -> Vec<PathBuf> {
    let direct: Vec<PathBuf> = bases
        .iter()
        .flat_map(|base| {
            [
                base.join(STEAM_SETTINGS),
                base.join("coldclient").join(STEAM_SETTINGS),
            ]
        })
        .collect();
    let found = contained_unique(root, direct);
    if !found.is_empty() || !deep_search {
        return found;
    }
    contained_unique(root, deep_find(root))
}

fn deep_find(root: &Path) -> Vec<PathBuf> {
    let mut level: VecDeque<PathBuf> = VecDeque::from([root.to_path_buf()]);
    let mut visited = 0;
    for _ in 1..=MAX_DEPTH {
        let mut next = VecDeque::new();
        let mut hits = Vec::new();
        while let Some(dir) = level.pop_front() {
            for child in plain_subdirs(&dir) {
                visited += 1;
                if visited > MAX_VISITED {
                    return hits;
                }
                if name_lower(&child) == STEAM_SETTINGS {
                    hits.push(child);
                } else if !name_in(&child, &IGNORED_DIRS) {
                    next.push_back(child);
                }
            }
        }
        if !hits.is_empty() {
            return hits;
        }
        level = next;
    }
    Vec::new()
}

fn plain_subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_type()
                .is_ok_and(|t| t.is_dir() && !t.is_symlink())
        })
        .map(|entry| entry.path())
        .collect();
    dirs.sort_by_key(|path| name_lower(path));
    dirs
}

fn contained_unique(root: &Path, candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    let Ok(root_real) = std::fs::canonicalize(root) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| candidate.is_dir())
        .filter(|candidate| {
            std::fs::canonicalize(candidate).is_ok_and(|real| {
                real.starts_with(&root_real) && seen.insert(real.to_string_lossy().to_lowercase())
            })
        })
        .collect()
}

fn name_lower(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn name_in(path: &Path, names: &[&str]) -> bool {
    let name = name_lower(path);
    names.iter().any(|candidate| *candidate == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkdirs(root: &Path, rels: &[&str]) {
        for rel in rels {
            std::fs::create_dir_all(root.join(rel)).unwrap();
        }
    }

    #[test]
    fn bases_cover_nested_unreal_unity_and_steamworks_layouts() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        mkdirs(
            r,
            &[
                "bin64",
                "MyGame/Binaries/Win64",
                "Game_Data/Plugins/x86_64",
                "Engine/Binaries/ThirdParty/Steamworks/Steamv157/Win64",
                "Docs",
            ],
        );
        let bases = base_dirs(r);
        assert_eq!(
            bases,
            vec![
                r.to_path_buf(),
                r.join("bin64"),
                r.join("MyGame").join("Binaries").join("Win64"),
                r.join("Game_Data").join("Plugins").join("x86_64"),
                r.join("Engine")
                    .join("Binaries")
                    .join("ThirdParty")
                    .join("Steamworks")
                    .join("Steamv157")
                    .join("Win64"),
            ]
        );
    }

    #[test]
    fn finds_steam_settings_next_to_bases_and_in_coldclient() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        mkdirs(r, &["bin64/steam_settings", "coldclient/steam_settings"]);
        let bases = base_dirs(r);
        let found = steam_settings_dirs(r, &bases, true);
        assert_eq!(
            found,
            vec![
                r.join("coldclient").join("steam_settings"),
                r.join("bin64").join("steam_settings")
            ]
        );
    }

    #[test]
    fn deep_search_finds_nested_settings_but_skips_ignored_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        mkdirs(r, &["a/b/c/steam_settings", "Content/x/steam_settings"]);
        let found = steam_settings_dirs(r, &base_dirs(r), true);
        assert_eq!(
            found,
            vec![r.join("a").join("b").join("c").join("steam_settings")]
        );
    }

    #[test]
    fn deep_search_is_off_when_not_requested() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        mkdirs(r, &["a/b/steam_settings"]);
        assert!(steam_settings_dirs(r, &base_dirs(r), false).is_empty());
    }

    #[test]
    fn deep_search_stops_at_depth_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        mkdirs(r, &["1/2/3/4/5/6/7/8/steam_settings"]);
        assert!(steam_settings_dirs(r, &base_dirs(r), true).is_empty());
        mkdirs(r, &["a/b/c/d/e/f/g/steam_settings"]);
        assert_eq!(steam_settings_dirs(r, &base_dirs(r), true).len(), 1);
    }

    #[test]
    fn deep_search_stops_after_visit_budget() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        for i in 0..(MAX_VISITED + 5) {
            std::fs::create_dir_all(r.join(format!("d{i:05}"))).unwrap();
        }
        mkdirs(r, &["zzzz/steam_settings"]);
        assert!(steam_settings_dirs(r, &base_dirs(r), true).is_empty());
    }
}
