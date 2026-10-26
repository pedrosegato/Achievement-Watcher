use std::collections::HashSet;
use std::path::PathBuf;

use crate::discovery::folders::KnownFolders;
use crate::discovery::gamedirs::collect_game_dirs;
use crate::discovery::gamediscovery::{GameDiscovery, Unresolved, discover_game_dirs};
use crate::discovery::sources::builtin_sources;
use crate::discovery::steam::{library_folders, normalize_for_compare, stats_files};
use crate::discovery::{Found, discover};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetectConfig {
    pub steam: Option<PathBuf>,
    pub steam_libraries: bool,
    pub steam_stats: bool,
    pub games_roots: Vec<PathBuf>,
    pub game_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detection {
    pub found: Vec<Found>,
    pub unresolved: Vec<Unresolved>,
}

pub fn detect(folders: &KnownFolders, config: &DetectConfig) -> Detection {
    let in_games = detect_in_game_folders(folders, config);
    Detection {
        found: merge_found(discover(builtin_sources(), folders), in_games.found),
        unresolved: in_games.unresolved,
    }
}

pub fn detect_in_game_folders(folders: &KnownFolders, config: &DetectConfig) -> GameDiscovery {
    let libraries = config
        .steam
        .as_deref()
        .filter(|_| config.steam_libraries)
        .map(library_folders)
        .unwrap_or_default();
    let games = collect_game_dirs(&libraries, &config.games_roots, &config.game_dirs);
    let mut discovery = discover_game_dirs(&games, folders);
    if config.steam_stats
        && let Some(steam) = config.steam.as_deref()
    {
        discovery.found.extend(stats_files(steam));
    }
    discovery
}

pub fn merge_found(first: Vec<Found>, second: Vec<Found>) -> Vec<Found> {
    let mut seen = HashSet::new();
    let mut found: Vec<Found> = first
        .into_iter()
        .chain(second)
        .filter(|f| seen.insert(normalize_for_compare(&f.path)))
        .collect();
    found.sort_by(|a, b| (a.app_id, a.source_id, &a.path).cmp(&(b.app_id, b.source_id, &b.path)));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn touch(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }

    #[test]
    fn joins_fixed_folders_and_game_folders() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            tmp.path(),
            "PublicDocuments/OnlineFix/10/Stats/Achievements.ini",
            "[A]\nachieved=true\n",
        );
        let roots = tmp.path().join("Games");
        touch(&roots, "G/steam_appid.txt", "20");
        touch(&roots, "G/SteamData/user_stats.ini", "[ACHIEVEMENTS]\n");
        let d = detect(
            &KnownFolders::under(tmp.path()),
            &DetectConfig {
                games_roots: vec![roots],
                ..DetectConfig::default()
            },
        );
        let got: Vec<(u32, &str)> = d.found.iter().map(|f| (f.app_id, f.source_id)).collect();
        assert_eq!(got, vec![(10, "onlinefix"), (20, "tenoke")]);
    }
}
