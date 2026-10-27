use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cloud::key::GameKind;
use crate::cloud::manifest::ManifestGame;
use crate::cloud::portable::{Placeholders, PortablePath};

const STEAM_SOURCE: &str = "steam";
const LUMAPLAY_SOURCE: &str = "lumaplay";
const REMOTE: &str = "remote";
const ROOT: &str = "<root>";
const USERDATA_PREFIX: &str = "<root>/userdata/";
const ROOT_GAME: &str = "<root>/<game>";
const BASE: &str = "<base>";
const STEAM_ROOT: &str = "<steamRoot>";
const ACCOUNT_TOKENS: [&str; 2] = ["<storeUserId>", "<storeGameId>"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Save,
    EmulatorFiles,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRoot {
    pub category: Category,
    pub pattern: PortablePath,
    pub single_file: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Located {
    Roots(Vec<SaveRoot>),
    RegistryOnly,
    Unsupported,
    NotFound,
}

pub struct LocateInput<'a> {
    pub app_id: u32,
    pub kind: GameKind,
    pub manifest: Option<&'a ManifestGame>,
    pub emulator_files: &'a [PathBuf],
    pub sources: &'a [String],
    pub manual: Option<&'a PortablePath>,
    pub placeholders: &'a Placeholders,
}

pub fn locate(input: &LocateInput) -> Located {
    let emulator_sources: Vec<&String> = input
        .sources
        .iter()
        .filter(|s| s.as_str() != STEAM_SOURCE)
        .collect();
    if !emulator_sources.is_empty() && emulator_sources.iter().all(|s| *s == LUMAPLAY_SOURCE) {
        return Located::Unsupported;
    }
    let emulator_dirs: Vec<SaveRoot> = if input.kind == GameKind::Emulator {
        dedupe(
            input
                .emulator_files
                .iter()
                .filter_map(|file| emulator_dir_for(file, input.app_id, input.placeholders))
                .collect(),
        )
    } else {
        Vec::new()
    };
    let folders: Vec<&PortablePath> = emulator_dirs
        .iter()
        .filter(|root| !root.single_file)
        .map(|root| &root.pattern)
        .collect();
    let mut saves: Vec<PortablePath> = Vec::new();
    if let Some(manual) = input.manual {
        saves.push(manual.clone());
    } else if let Some(manifest) = input.manifest {
        let has_base = input.placeholders.install_dir.is_some();
        for file in &manifest.files {
            saves.extend(convert(&file.pattern, input.kind, &folders, has_base));
        }
    }
    saves.extend(folders.iter().map(|dir| dir.join(REMOTE)));
    let mut roots: Vec<SaveRoot> = dedupe(
        saves
            .into_iter()
            .map(|pattern| SaveRoot {
                category: Category::Save,
                pattern,
                single_file: false,
            })
            .collect(),
    );
    roots.extend(emulator_dirs);
    if roots.is_empty() {
        let registry = input
            .manifest
            .is_some_and(|m| m.has_registry && m.files.is_empty());
        return if registry {
            Located::RegistryOnly
        } else {
            Located::NotFound
        };
    }
    Located::Roots(roots)
}

fn convert(
    pattern: &str,
    kind: GameKind,
    emulator_dirs: &[&PortablePath],
    has_base: bool,
) -> Vec<PortablePath> {
    let text = pattern.to_owned();
    let candidates: Vec<String> = if let Some(rest) = text.strip_prefix(USERDATA_PREFIX) {
        match kind {
            GameKind::SteamOriginal => vec![format!("{STEAM_ROOT}/userdata/{rest}")],
            GameKind::Emulator => {
                let mut parts = rest.split('/');
                let (_, _, remote) = (parts.next(), parts.next(), parts.next());
                if remote != Some(REMOTE) {
                    return Vec::new();
                }
                let tail: Vec<&str> = parts.collect();
                emulator_dirs
                    .iter()
                    .map(|dir| dir.join(REMOTE).join(&tail.join("/")).as_str().to_owned())
                    .collect()
            }
        }
    } else if let Some(rest) = text.strip_prefix(ROOT_GAME) {
        vec![format!("{BASE}{rest}")]
    } else if text.starts_with(ROOT) {
        Vec::new()
    } else {
        vec![text]
    };
    candidates
        .into_iter()
        .filter(|text| has_base || !text.starts_with(BASE))
        .filter(|text| {
            text.split_once('/').is_none_or(|(_, rest)| {
                !ACCOUNT_TOKENS
                    .iter()
                    .fold(rest.to_owned(), |left, token| left.replace(token, ""))
                    .contains('<')
            })
        })
        .filter_map(|text| PortablePath::new(&text))
        .collect()
}

fn dedupe<T: Clone + HasPattern>(items: Vec<T>) -> Vec<T> {
    let mut kept: Vec<T> = Vec::new();
    for item in items {
        let inside = kept
            .iter()
            .any(|other| contains(other.pattern(), item.pattern()));
        if !inside {
            kept.retain(|other| !contains(item.pattern(), other.pattern()));
            kept.push(item);
        }
    }
    kept
}

trait HasPattern {
    fn pattern(&self) -> &str;
}

impl HasPattern for SaveRoot {
    fn pattern(&self) -> &str {
        self.pattern.as_str()
    }
}

fn contains(outer: &str, inner: &str) -> bool {
    inner == outer || inner.starts_with(&format!("{outer}/"))
}

pub fn emulator_dir_for(file: &Path, app_id: u32, ph: &Placeholders) -> Option<SaveRoot> {
    let name = app_id.to_string();
    let folder = file
        .ancestors()
        .skip(1)
        .filter(|dir| dir.file_name().is_some_and(|n| n.to_string_lossy() == name))
        .find_map(|dir| PortablePath::from_absolute(dir, ph).filter(|p| p.as_str().contains('/')));
    match folder {
        Some(pattern) => Some(SaveRoot {
            category: Category::EmulatorFiles,
            pattern,
            single_file: false,
        }),
        None => PortablePath::from_absolute(file, ph).map(|pattern| SaveRoot {
            category: Category::EmulatorFiles,
            pattern,
            single_file: true,
        }),
    }
}

pub fn expand(root: &SaveRoot, ph: &Placeholders) -> Vec<PathBuf> {
    let mut segments = root.pattern.as_str().split('/');
    let Some(base) = segments
        .next()
        .and_then(PortablePath::new)
        .and_then(|token| token.to_absolute(ph))
    else {
        return Vec::new();
    };
    let mut current = vec![base];
    for segment in segments.filter(|s| !s.is_empty()) {
        current = if is_account_segment(segment) {
            current
                .iter()
                .flat_map(|dir| {
                    std::fs::read_dir(dir)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
                        .map(|entry| entry.path())
                        .collect::<Vec<_>>()
                })
                .collect()
        } else if segment.contains('*') {
            current
                .iter()
                .flat_map(|dir| {
                    std::fs::read_dir(dir)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .filter(|entry| {
                            wildcard_match(segment, &entry.file_name().to_string_lossy())
                        })
                        .map(|entry| entry.path())
                        .collect::<Vec<_>>()
                })
                .collect()
        } else {
            current.iter().map(|dir| dir.join(segment)).collect()
        };
    }
    let mut found: Vec<PathBuf> = current.into_iter().filter(|path| path.exists()).collect();
    found.sort();
    found
}

pub(crate) fn is_account_segment(segment: &str) -> bool {
    ACCOUNT_TOKENS.contains(&segment)
}

pub(crate) fn wildcard_match(pattern: &str, name: &str) -> bool {
    if is_account_segment(pattern) {
        return true;
    }
    let pattern = pattern.to_lowercase();
    let name = name.to_lowercase();
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, rest) = parts.split_first().unwrap_or((&"", &[]));
    let Some(mut remaining) = name.strip_prefix(first) else {
        return false;
    };
    let Some((last, middle)) = rest.split_last() else {
        return remaining.is_empty();
    };
    for part in middle {
        match remaining.find(part) {
            Some(index) => remaining = &remaining[index + part.len()..],
            None => return false,
        }
    }
    remaining.len() >= last.len() && remaining.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::manifest::parse_manifest;
    use crate::discovery::folders::KnownFolders;

    const SAMPLE: &str = include_str!("../../tests/ludusavi/manifest-sample.yaml");

    fn manifest(app_id: u32) -> ManifestGame {
        parse_manifest(SAMPLE).unwrap().games[&app_id].clone()
    }

    fn ph(root: &Path) -> Placeholders {
        Placeholders {
            folders: Some(KnownFolders::under(root)),
            ..Default::default()
        }
    }

    fn roots(located: Located) -> Vec<(Category, String, bool)> {
        match located {
            Located::Roots(roots) => roots
                .into_iter()
                .map(|r| (r.category, r.pattern.as_str().to_owned(), r.single_file))
                .collect(),
            other => panic!("expected roots, got {other:?}"),
        }
    }

    fn input<'a>(
        app_id: u32,
        kind: GameKind,
        manifest: Option<&'a ManifestGame>,
        emulator_files: &'a [PathBuf],
        sources: &'a [String],
        placeholders: &'a Placeholders,
    ) -> LocateInput<'a> {
        LocateInput {
            app_id,
            kind,
            manifest,
            emulator_files,
            sources,
            manual: None,
            placeholders,
        }
    }

    #[test]
    fn ds3_locates_under_app_data_with_wildcard() {
        let dir = tempfile::tempdir().unwrap();
        let ph = ph(dir.path());
        let ds3 = manifest(374320);
        let got = locate(&input(
            374320,
            GameKind::SteamOriginal,
            Some(&ds3),
            &[],
            &[],
            &ph,
        ));
        assert_eq!(
            roots(got),
            vec![(
                Category::Save,
                "<winAppData>/DarkSoulsIII/<storeUserId>".to_owned(),
                false
            )]
        );
    }

    #[test]
    fn steam_original_maps_userdata_to_steam_root() {
        let dir = tempfile::tempdir().unwrap();
        let ph = ph(dir.path());
        let hack = manifest(525480);
        let got = locate(&input(
            525480,
            GameKind::SteamOriginal,
            Some(&hack),
            &[],
            &[],
            &ph,
        ));
        assert_eq!(
            roots(got),
            vec![(
                Category::Save,
                "<steamRoot>/userdata/<storeUserId>/525480/remote/savedata".to_owned(),
                false
            )]
        );
    }

    #[test]
    fn emulator_game_maps_userdata_remote_to_emulator_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ph = ph(dir.path());
        let hack = manifest(525480);
        let file = dir
            .path()
            .join("RoamingAppData")
            .join("GSE Saves")
            .join("525480")
            .join("achievements.json");
        let files = [file];
        let sources = ["gse".to_owned()];
        let got = locate(&input(
            525480,
            GameKind::Emulator,
            Some(&hack),
            &files,
            &sources,
            &ph,
        ));
        assert_eq!(
            roots(got),
            vec![
                (
                    Category::Save,
                    "<winAppData>/GSE Saves/525480/remote".to_owned(),
                    false
                ),
                (
                    Category::EmulatorFiles,
                    "<winAppData>/GSE Saves/525480".to_owned(),
                    false
                ),
            ]
        );
    }

    #[test]
    fn in_game_emulator_file_is_a_single_file_root() {
        let dir = tempfile::tempdir().unwrap();
        let mut ph = ph(dir.path());
        let game = dir.path().join("Games").join("Thing");
        ph.install_dir = Some(game.clone());
        let files = [game.join("steam_settings").join("achievements.json")];
        let sources = ["gse_settings".to_owned()];
        let got = locate(&input(42, GameKind::Emulator, None, &files, &sources, &ph));
        assert_eq!(
            roots(got),
            vec![(
                Category::EmulatorFiles,
                "<base>/steam_settings/achievements.json".to_owned(),
                true
            )]
        );
    }

    #[test]
    fn manual_path_replaces_manifest_saves() {
        let dir = tempfile::tempdir().unwrap();
        let ph = ph(dir.path());
        let ds3 = manifest(374320);
        let manual = PortablePath::new("<winDocuments>/My DS3").unwrap();
        let mut request = input(374320, GameKind::SteamOriginal, Some(&ds3), &[], &[], &ph);
        request.manual = Some(&manual);
        assert_eq!(
            roots(locate(&request)),
            vec![(Category::Save, "<winDocuments>/My DS3".to_owned(), false)]
        );
    }

    #[test]
    fn registry_only_game_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let ph = ph(dir.path());
        let game = manifest(449940);
        let got = locate(&input(
            449940,
            GameKind::SteamOriginal,
            Some(&game),
            &[],
            &[],
            &ph,
        ));
        assert_eq!(got, Located::RegistryOnly);
    }

    #[test]
    fn lumaplay_only_game_is_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let ph = ph(dir.path());
        let sources = ["lumaplay".to_owned()];
        let got = locate(&input(7, GameKind::Emulator, None, &[], &sources, &ph));
        assert_eq!(got, Located::Unsupported);
    }

    #[test]
    fn unknown_game_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let ph = ph(dir.path());
        let got = locate(&input(7, GameKind::SteamOriginal, None, &[], &[], &ph));
        assert_eq!(got, Located::NotFound);
    }

    #[test]
    fn base_paths_need_an_install_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut ph = ph(dir.path());
        let gnorp = manifest(1473350);
        let without = roots(locate(&input(
            1473350,
            GameKind::SteamOriginal,
            Some(&gnorp),
            &[],
            &[],
            &ph,
        )));
        assert_eq!(
            without,
            vec![(
                Category::Save,
                "<winAppData>/Gnorp/Apologue/Saves".to_owned(),
                false
            )]
        );
        ph.install_dir = Some(dir.path().join("Gnorp"));
        let with = roots(locate(&input(
            1473350,
            GameKind::SteamOriginal,
            Some(&gnorp),
            &[],
            &[],
            &ph,
        )));
        assert_eq!(with.len(), 2);
        assert!(with.contains(&(Category::Save, "<base>/saves".to_owned(), false)));
    }

    #[test]
    fn expand_matches_star_inside_segments() {
        let dir = tempfile::tempdir().unwrap();
        let ph = ph(dir.path());
        let game = dir.path().join("RoamingAppData").join("Game");
        for user in ["111", "222"] {
            std::fs::create_dir_all(game.join(user)).unwrap();
            std::fs::write(game.join(user).join("save1.dat"), "x").unwrap();
            std::fs::write(game.join(user).join("other.txt"), "x").unwrap();
        }
        let root = SaveRoot {
            category: Category::Save,
            pattern: PortablePath::new("<winAppData>/Game/*/save*.dat").unwrap(),
            single_file: false,
        };
        let mut got = expand(&root, &ph);
        got.sort();
        assert_eq!(
            got,
            vec![
                game.join("111").join("save1.dat"),
                game.join("222").join("save1.dat")
            ]
        );
    }

    #[test]
    fn expand_skips_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let ph = ph(dir.path());
        let root = SaveRoot {
            category: Category::Save,
            pattern: PortablePath::new("<winAppData>/Nothing").unwrap(),
            single_file: false,
        };
        assert!(expand(&root, &ph).is_empty());
    }

    #[test]
    fn account_segment_matches_folders_only() {
        let dir = tempfile::tempdir().unwrap();
        let ph = ph(dir.path());
        let game = dir.path().join("RoamingAppData").join("DarkSoulsIII");
        std::fs::create_dir_all(game.join("0110000100000001")).unwrap();
        std::fs::write(game.join("GraphicsConfig.xml"), "<x/>").unwrap();
        let root = SaveRoot {
            category: Category::Save,
            pattern: PortablePath::new("<winAppData>/DarkSoulsIII/<storeUserId>").unwrap(),
            single_file: false,
        };
        assert_eq!(expand(&root, &ph), vec![game.join("0110000100000001")]);
    }
}
