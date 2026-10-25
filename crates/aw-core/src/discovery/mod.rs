pub mod appinfo;
pub mod detect;
pub mod folders;
pub mod gamedirs;
pub mod gamediscovery;
pub mod gamefolder;
pub mod markers;
pub mod running;
pub mod sources;
pub mod steam;

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::discovery::folders::KnownFolders;
use crate::discovery::sources::SourceDef;
use crate::parsers::Format;

const APPID_SEGMENT: &str = "{appid}";
const WILDCARD_SEGMENT: &str = "*";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub source_id: &'static str,
    pub app_id: u32,
    pub path: PathBuf,
    pub format: Format,
}

pub fn discover(sources: &[SourceDef], folders: &KnownFolders) -> Vec<Found> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for source in sources {
        let Some(base) = folders.resolve(source.base) else {
            continue;
        };
        let root = source
            .root
            .split('/')
            .fold(base.to_path_buf(), |dir, segment| dir.join(segment));
        for pattern in source.patterns {
            for (app_id, path) in match_pattern(&root, pattern) {
                if seen.insert(path.clone()) {
                    found.push(Found {
                        source_id: source.id,
                        app_id,
                        path,
                        format: source.format,
                    });
                }
            }
        }
    }
    found.sort_by(|a, b| (a.app_id, a.source_id, &a.path).cmp(&(b.app_id, b.source_id, &b.path)));
    found
}

fn match_pattern(root: &Path, pattern: &str) -> Vec<(u32, PathBuf)> {
    let segments: Vec<&str> = pattern.split('/').collect();
    let Some((file_name, dir_segments)) = segments.split_last() else {
        return Vec::new();
    };
    let mut frontier = vec![(root.to_path_buf(), None)];
    for segment in dir_segments {
        frontier = frontier
            .into_iter()
            .flat_map(|(dir, app_id)| matching_subdirs(&dir, segment, app_id))
            .collect();
    }
    frontier
        .into_iter()
        .filter_map(|(dir, app_id)| Some((dir, app_id?)))
        .flat_map(|(dir, app_id)| {
            matching_files(&dir, file_name)
                .into_iter()
                .map(move |path| (app_id, path))
        })
        .collect()
}

fn entries(dir: &Path) -> impl Iterator<Item = (OsString, PathBuf)> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| (entry.file_name(), entry.path()))
}

fn matching_subdirs(dir: &Path, segment: &str, app_id: Option<u32>) -> Vec<(PathBuf, Option<u32>)> {
    entries(dir)
        .filter(|(_, path)| path.is_dir())
        .filter_map(|(name, path)| match segment {
            APPID_SEGMENT => {
                let digits = parse_app_id(&name)?;
                app_id
                    .is_none_or(|expected| expected == digits)
                    .then_some((path, Some(digits)))
            }
            WILDCARD_SEGMENT => Some((path, app_id)),
            literal => name.eq_ignore_ascii_case(literal).then_some((path, app_id)),
        })
        .collect()
}

fn matching_files(dir: &Path, file_name: &str) -> Vec<PathBuf> {
    entries(dir)
        .filter(|(name, path)| name.eq_ignore_ascii_case(file_name) && path.is_file())
        .map(|(_, path)| path)
        .collect()
}

fn parse_app_id(name: &OsStr) -> Option<u32> {
    let name = name.to_str()?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    name.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::folders::Base;
    use crate::discovery::sources::{SourceDef, builtin_sources};
    use std::path::{Path, PathBuf};

    static WILDCARD_SOURCES: [SourceDef; 1] = [SourceDef {
        id: "wild",
        name: "Wild",
        base: Base::ProgramData,
        root: "Steam",
        patterns: &["*/{appid}/stats/achievements.ini"],
        format: Format::CodexIni,
    }];

    fn touch(root: &Path, rel: &str) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"x").unwrap();
        p
    }

    #[test]
    fn wildcard_segment_matches_any_directory() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            tmp.path(),
            "ProgramData/Steam/Player/10/stats/achievements.ini",
        );
        touch(
            tmp.path(),
            "ProgramData/Steam/RLD!/20/stats/achievements.ini",
        );
        touch(tmp.path(), "ProgramData/Steam/30/stats/achievements.ini");
        let found = discover(&WILDCARD_SOURCES, &KnownFolders::under(tmp.path()));
        let ids: Vec<u32> = found.iter().map(|f| f.app_id).collect();
        assert_eq!(ids, vec![10, 20]);
    }

    #[test]
    fn finds_new_fixed_folder_sources_and_ignores_unrelated_player_folders() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            tmp.path(),
            "ProgramData/Steam/Player/101/stats/achievements.ini",
        );
        touch(tmp.path(), "ProgramData/RLD!/102/achievements.ini");
        touch(
            tmp.path(),
            "Documents/SKIDROW/103/SteamEmu/UserStats/achiev.ini",
        );
        touch(
            tmp.path(),
            "Documents/Player/104/SteamEmu/UserStats/achiev.ini",
        );
        touch(
            tmp.path(),
            "LocalAppData/SKIDROW/105/SteamEmu/UserStats/achiev.ini",
        );
        touch(
            tmp.path(),
            "RoamingAppData/CreamAPI/106/stats/CreamAPI.Achievements.cfg",
        );
        touch(tmp.path(), "Documents/Player/Saves/slot1.sav");
        touch(tmp.path(), "Documents/Player/107/profile.ini");
        touch(
            tmp.path(),
            "Documents/Player/Saves/SteamEmu/UserStats/achiev.ini",
        );
        touch(
            tmp.path(),
            "Documents/Player/108/SteamEmu/UserStats/other.ini",
        );
        touch(tmp.path(), "ProgramData/Steam/30/stats/achievements.ini");
        touch(
            tmp.path(),
            "ProgramData/Steam/A/B/109/stats/achievements.ini",
        );
        let found = discover(builtin_sources(), &KnownFolders::under(tmp.path()));
        let got: Vec<(u32, &str, Format)> = found
            .iter()
            .map(|f| (f.app_id, f.source_id, f.format))
            .collect();
        assert_eq!(
            got,
            vec![
                (101, "reloaded_programdata", Format::ReloadedIni),
                (102, "reloaded_rld", Format::ReloadedIni),
                (103, "skidrow_documents", Format::AchievIni),
                (104, "skidrow_player", Format::AchievIni),
                (105, "skidrow_local", Format::AchievIni),
                (106, "creamapi", Format::CreamApi),
            ]
        );
    }

    #[test]
    fn missing_base_skips_only_its_sources() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            tmp.path(),
            "PublicDocuments/OnlineFix/10/Stats/Achievements.ini",
        );
        touch(
            tmp.path(),
            "Documents/SKIDROW/20/SteamEmu/UserStats/achiev.ini",
        );
        touch(
            tmp.path(),
            "LocalAppData/SKIDROW/30/SteamEmu/UserStats/achiev.ini",
        );
        let folders = KnownFolders {
            documents: None,
            ..KnownFolders::under(tmp.path())
        };
        let found = discover(builtin_sources(), &folders);
        let got: Vec<(u32, &str)> = found.iter().map(|f| (f.app_id, f.source_id)).collect();
        assert_eq!(got, vec![(10, "onlinefix"), (30, "skidrow_local")]);
    }

    #[test]
    fn finds_onlinefix_file_case_insensitively_and_ignores_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let hit = touch(
            tmp.path(),
            "PublicDocuments/OnlineFix/1245620/Stats/achievements.ini",
        );
        touch(
            tmp.path(),
            "PublicDocuments/OnlineFix/Saves/achievements.ini",
        );
        std::fs::create_dir_all(tmp.path().join("PublicDocuments/OnlineFix/2622380/Saves"))
            .unwrap();
        let found = discover(builtin_sources(), &KnownFolders::under(tmp.path()));
        assert_eq!(
            found,
            vec![Found {
                source_id: "onlinefix",
                app_id: 1245620,
                path: hit,
                format: Format::OnlineFix
            }]
        );
    }

    #[test]
    fn empress_requires_matching_appids() {
        let tmp = tempfile::tempdir().unwrap();
        let hit = touch(
            tmp.path(),
            "PublicDocuments/EMPRESS/111/remote/111/achievements.json",
        );
        touch(
            tmp.path(),
            "PublicDocuments/EMPRESS/222/remote/333/achievements.json",
        );
        let found = discover(builtin_sources(), &KnownFolders::under(tmp.path()));
        assert_eq!(
            found
                .iter()
                .map(|f| (f.app_id, f.path.clone()))
                .collect::<Vec<_>>(),
            vec![(111, hit)]
        );
    }

    #[test]
    fn matches_directory_segments_case_insensitively() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            tmp.path(),
            "PublicDocuments/OnlineFix/7/stats/achievements.ini",
        );
        touch(
            tmp.path(),
            "PublicDocuments/EMPRESS/111/REMOTE/111/achievements.json",
        );
        let found = discover(builtin_sources(), &KnownFolders::under(tmp.path()));
        let app_ids: Vec<u32> = found.iter().map(|f| f.app_id).collect();
        assert_eq!(app_ids, vec![7, 111]);
    }

    #[test]
    fn results_are_sorted_by_appid_then_source() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "RoamingAppData/GSE Saves/480/achievements.json");
        touch(
            tmp.path(),
            "RoamingAppData/Goldberg SteamEmu Saves/480/achievements.json",
        );
        touch(tmp.path(), "PublicDocuments/OnlineFix/10/Achievements.ini");
        let found = discover(builtin_sources(), &KnownFolders::under(tmp.path()));
        let order: Vec<(u32, &str)> = found.iter().map(|f| (f.app_id, f.source_id)).collect();
        assert_eq!(
            order,
            vec![(10, "onlinefix"), (480, "goldberg"), (480, "gse")]
        );
    }

    #[test]
    fn discovers_under_non_ascii_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Usuário João");
        touch(&root, "RoamingAppData/GSE Saves/570/achievements.json");
        let found = discover(builtin_sources(), &KnownFolders::under(&root));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].app_id, 570);
    }

    #[test]
    fn missing_roots_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(discover(builtin_sources(), &KnownFolders::under(tmp.path())).is_empty());
    }

    #[test]
    fn a_file_matched_by_several_patterns_is_reported_once() {
        let tmp = tempfile::tempdir().unwrap();
        let hit = touch(tmp.path(), "PublicDocuments/Emu/42/Achievements.ini");
        let sources = [SourceDef {
            id: "emu",
            name: "Emu",
            base: Base::PublicDocuments,
            root: "Emu",
            patterns: &["{appid}/achievements.ini", "{appid}/ACHIEVEMENTS.INI"],
            format: Format::CodexIni,
        }];
        let found = discover(&sources, &KnownFolders::under(tmp.path()));
        assert_eq!(
            found,
            vec![Found {
                source_id: "emu",
                app_id: 42,
                path: hit,
                format: Format::CodexIni
            }]
        );
    }
}
