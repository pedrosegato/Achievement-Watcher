pub mod appinfo;
pub mod detect;
pub mod folders;
pub mod gamedirs;
pub mod gamediscovery;
pub mod gamefolder;
pub mod lumaplay;
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
use crate::platform::{ExternalId, Platform, valid_external_id};

const APPID_SEGMENT: &str = "{appid}";
const WILDCARD_SEGMENT: &str = "*";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub source_id: &'static str,
    pub app_id: u32,
    pub external: Option<ExternalId>,
    pub path: PathBuf,
    pub format: Format,
}

impl Found {
    pub fn steam(source_id: &'static str, app_id: u32, path: PathBuf, format: Format) -> Found {
        Found {
            source_id,
            app_id,
            external: None,
            path,
            format,
        }
    }

    pub fn external(
        source_id: &'static str,
        external: ExternalId,
        path: PathBuf,
        format: Format,
    ) -> Found {
        Found {
            source_id,
            app_id: 0,
            external: Some(external),
            path,
            format,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GameKey {
    Steam(u32),
    External(String),
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
            for (key, path) in match_pattern(&root, pattern, source.platform) {
                if seen.insert(path.clone()) {
                    found.push(match key {
                        GameKey::Steam(app_id) => {
                            Found::steam(source.id, app_id, path, source.format)
                        }
                        GameKey::External(id) => Found::external(
                            source.id,
                            ExternalId {
                                platform: source.platform,
                                id,
                            },
                            path,
                            source.format,
                        ),
                    });
                }
            }
        }
    }
    found.sort_by(|a, b| (a.app_id, a.source_id, &a.path).cmp(&(b.app_id, b.source_id, &b.path)));
    found
}

fn match_pattern(root: &Path, pattern: &str, platform: Platform) -> Vec<(GameKey, PathBuf)> {
    let segments: Vec<&str> = pattern.split('/').collect();
    let Some((file_name, dir_segments)) = segments.split_last() else {
        return Vec::new();
    };
    let mut frontier = vec![(root.to_path_buf(), None)];
    for segment in dir_segments {
        frontier = frontier
            .into_iter()
            .flat_map(|(dir, key)| matching_subdirs(&dir, segment, key, platform))
            .collect();
    }
    frontier
        .into_iter()
        .filter_map(|(dir, key)| Some((dir, key?)))
        .flat_map(|(dir, key)| {
            matching_files(&dir, file_name)
                .into_iter()
                .map(move |path| (key.clone(), path))
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

fn matching_subdirs(
    dir: &Path,
    segment: &str,
    key: Option<GameKey>,
    platform: Platform,
) -> Vec<(PathBuf, Option<GameKey>)> {
    entries(dir)
        .filter(|(_, path)| path.is_dir())
        .filter_map(|(name, path)| match segment {
            APPID_SEGMENT => {
                let here = game_key(&name, platform)?;
                key.as_ref()
                    .is_none_or(|expected| *expected == here)
                    .then_some((path, Some(here)))
            }
            WILDCARD_SEGMENT => Some((path, key.clone())),
            literal => name
                .eq_ignore_ascii_case(literal)
                .then(|| (path, key.clone())),
        })
        .collect()
}

fn game_key(name: &OsStr, platform: Platform) -> Option<GameKey> {
    match platform {
        Platform::Steam => parse_app_id(name).map(GameKey::Steam),
        _ => name
            .to_str()
            .filter(|name| valid_external_id(name))
            .map(|name| GameKey::External(name.to_owned())),
    }
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
    use crate::platform::Platform;
    use std::path::{Path, PathBuf};

    static WILDCARD_SOURCES: [SourceDef; 1] = [SourceDef {
        id: "wild",
        name: "Wild",
        platform: Platform::Steam,
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
                external: None,
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
            platform: Platform::Steam,
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
                external: None,
                path: hit,
                format: Format::CodexIni
            }]
        );
    }

    type Key = (&'static str, u32, Option<(Platform, String)>);

    fn keys(found: &[Found]) -> Vec<Key> {
        found
            .iter()
            .map(|f| {
                (
                    f.source_id,
                    f.app_id,
                    f.external.as_ref().map(|e| (e.platform, e.id.clone())),
                )
            })
            .collect()
    }

    #[test]
    fn finds_m7_sources() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            tmp.path(),
            "Documents/RUNE/Ubisoft Connect/achievements/Player/4/achievements.cfg",
        );
        touch(
            tmp.path(),
            "PublicDocuments/RUNE/Ubisoft Connect/achievements/Player/54/achievements.cfg",
        );
        touch(
            tmp.path(),
            "RoamingAppData/Goldberg UplayEmu Saves/635/achievements.json",
        );
        touch(
            tmp.path(),
            "RoamingAppData/R1 UplayEmu Saves/40/achievements.ini",
        );
        touch(
            tmp.path(),
            "LocalAppData/anadius/LSX emu/achievement_watcher/1238840/achievements.ini",
        );
        touch(
            tmp.path(),
            "RoamingAppData/NemirtingasEpicEmu/Player/9d2d0eb64d5c44529cece33fe2a46482/achievements.json",
        );
        touch(
            tmp.path(),
            "RoamingAppData/NemirtingasGalaxyEmu/Player/1207658924/achievements.json",
        );
        touch(
            tmp.path(),
            "LocalAppData/UniverseLAN/1495134320/UniverseLANData/Achievements.ini",
        );
        let found = discover(builtin_sources(), &KnownFolders::under(tmp.path()));
        let mut got = keys(&found);
        got.sort_by(|a, b| a.0.cmp(b.0));
        let ext = |platform: Platform, id: &str| Some((platform, id.to_owned()));
        assert_eq!(
            got,
            vec![
                ("anadius", 1238840, None),
                ("goldberg_uplay_json", 0, ext(Platform::Ubisoft, "635")),
                (
                    "nemirtingas_epic",
                    0,
                    ext(Platform::Epic, "9d2d0eb64d5c44529cece33fe2a46482")
                ),
                ("nemirtingas_galaxy", 0, ext(Platform::Gog, "1207658924")),
                ("r1_uplay_ini", 0, ext(Platform::Ubisoft, "40")),
                ("rune_uplay_documents", 0, ext(Platform::Ubisoft, "4")),
                ("rune_uplay_public", 0, ext(Platform::Ubisoft, "54")),
                ("universelan", 0, ext(Platform::Gog, "1495134320")),
            ]
        );
        let formats: Vec<(&str, Format)> = found.iter().map(|f| (f.source_id, f.format)).collect();
        assert!(formats.contains(&("rune_uplay_documents", Format::RuneCfg)));
        assert!(formats.contains(&("goldberg_uplay_json", Format::GenericJson)));
        assert!(formats.contains(&("r1_uplay_ini", Format::GenericIni)));
        assert!(formats.contains(&("anadius", Format::CodexIni)));
        assert!(formats.contains(&("universelan", Format::GenericIni)));
    }

    #[test]
    fn rejects_invalid_external_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let long = "a".repeat(65);
        for id in ["invalidappid", "0000", "a b", long.as_str()] {
            touch(
                tmp.path(),
                &format!("RoamingAppData/NemirtingasEpicEmu/Player/{id}/achievements.json"),
            );
        }
        assert!(discover(builtin_sources(), &KnownFolders::under(tmp.path())).is_empty());
    }

    #[test]
    fn external_segment_keeps_case() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            tmp.path(),
            "RoamingAppData/NemirtingasEpicEmu/Player/Carnation/achievements.json",
        );
        let found = discover(builtin_sources(), &KnownFolders::under(tmp.path()));
        assert_eq!(found[0].external.as_ref().unwrap().id, "Carnation");
    }

    #[test]
    fn steam_found_constructor_has_no_external_id() {
        let f = Found::steam("goldberg", 480, PathBuf::from("x"), Format::GoldbergJson);
        assert_eq!(f.external, None);
        assert_eq!(f.app_id, 480);
    }
}
