use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::discovery::steam::{installed_apps, normalize_for_compare};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameDir {
    pub path: PathBuf,
    pub app_id_hint: Option<u32>,
    pub from_steam_manifest: bool,
}

pub fn collect_game_dirs(
    libraries: &[PathBuf],
    games_roots: &[PathBuf],
    game_dirs: &[PathBuf],
) -> Vec<GameDir> {
    let mut candidates = Vec::new();
    for library in libraries {
        candidates.extend(installed_apps(library).into_iter().map(|app| GameDir {
            path: app.install_dir,
            app_id_hint: Some(app.app_id),
            from_steam_manifest: true,
        }));
    }
    for library in libraries {
        candidates.extend(
            subdirs(&library.join("steamapps").join("common"))
                .into_iter()
                .map(plain),
        );
    }
    for root in games_roots {
        candidates.extend(subdirs(root).into_iter().map(plain));
    }
    candidates.extend(
        game_dirs
            .iter()
            .filter(|dir| dir.is_dir())
            .cloned()
            .map(plain),
    );
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|game| seen.insert(normalize_for_compare(&game.path)))
        .collect()
}

fn plain(path: PathBuf) -> GameDir {
    GameDir {
        path,
        app_id_hint: None,
        from_steam_manifest: false,
    }
}

pub(crate) fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().to_lowercase(),
                entry.path(),
            )
        })
        .collect();
    dirs.sort();
    dirs.into_iter().map(|(_, path)| path).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(root: &Path, rel: &str, body: &str) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn collects_manifest_games_then_common_then_roots_then_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = tmp.path().join("Lib");
        touch(
            &lib,
            "steamapps/appmanifest_480.acf",
            "\"AppState\" { \"appid\" \"480\" \"name\" \"S\" \"StateFlags\" \"4\" \"installdir\" \"Spacewar\" }",
        );
        touch(&lib, "steamapps/common/Spacewar/a.exe", "");
        touch(&lib, "steamapps/common/Cracked/b.exe", "");
        let roots = tmp.path().join("Jogos");
        touch(&roots, "ELDEN RING/Game/eldenring.exe", "");
        touch(&roots, "Other/x.exe", "");
        let extra = tmp.path().join("Loose");
        std::fs::create_dir_all(&extra).unwrap();
        let dup = PathBuf::from(roots.join("OTHER").display().to_string());
        let games = collect_game_dirs(
            std::slice::from_ref(&lib),
            std::slice::from_ref(&roots),
            &[extra.clone(), dup],
        );
        let got: Vec<(PathBuf, Option<u32>, bool)> = games
            .into_iter()
            .map(|g| (g.path, g.app_id_hint, g.from_steam_manifest))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    lib.join("steamapps").join("common").join("Spacewar"),
                    Some(480),
                    true
                ),
                (
                    lib.join("steamapps").join("common").join("Cracked"),
                    None,
                    false
                ),
                (roots.join("ELDEN RING"), None, false),
                (roots.join("Other"), None, false),
                (extra, None, false),
            ]
        );
    }

    #[test]
    fn missing_inputs_yield_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            collect_game_dirs(
                &[tmp.path().join("x")],
                &[tmp.path().join("y")],
                &[tmp.path().join("z")]
            )
            .is_empty()
        );
    }
}
