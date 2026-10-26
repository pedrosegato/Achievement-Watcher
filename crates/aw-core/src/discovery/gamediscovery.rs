use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::discovery::Found;
use crate::discovery::folders::{Base, KnownFolders};
use crate::discovery::gamedirs::{GameDir, subdirs};
use crate::discovery::gamefolder::{base_dirs, steam_settings_dirs};
use crate::discovery::markers::scan_markers;
use crate::discovery::running::{KnownGame, app_id_in};
use crate::parsers::Format;
use crate::parsers::ini::Ini;
use crate::parsers::text;

const DEFAULT_SAVE_FOLDERS: [&str; 2] = ["GSE Saves", "Goldberg SteamEmu Saves"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Unresolved {
    pub source_id: &'static str,
    pub path: PathBuf,
    pub reason: &'static str,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GameDiscovery {
    pub found: Vec<Found>,
    pub unresolved: Vec<Unresolved>,
    pub games: Vec<KnownGame>,
}

pub fn discover_game_dirs(games: &[GameDir], folders: &KnownFolders) -> GameDiscovery {
    let mut result = GameDiscovery::default();
    for game in games {
        discover_game(game, folders, &mut result);
    }
    let mut seen = HashSet::new();
    result.found.retain(|found| seen.insert(found.path.clone()));
    result
        .found
        .sort_by(|a, b| (a.app_id, a.source_id, &a.path).cmp(&(b.app_id, b.source_id, &b.path)));
    result.unresolved.sort_by(|a, b| a.path.cmp(&b.path));
    result
}

fn discover_game(game: &GameDir, folders: &KnownFolders, result: &mut GameDiscovery) {
    let bases = base_dirs(&game.path);
    let settings = steam_settings_dirs(&game.path, &bases, !game.from_steam_manifest);
    let app_id = game
        .app_id_hint
        .or_else(|| bases.iter().find_map(|base| tenoke_app_id(base)))
        .or_else(|| settings.iter().find_map(|dir| steam_appid_txt(dir)))
        .or_else(|| bases.iter().find_map(|base| steam_appid_txt(base)))
        .or_else(|| bases.iter().find_map(|base| app_id_in(base)));
    if let Some(app_id) = app_id {
        result.games.push(KnownGame {
            dir: game.path.clone(),
            app_id,
        });
    }
    for base in &bases {
        let stats = base.join("SteamData").join("user_stats.ini");
        if !stats.is_file() {
            continue;
        }
        match app_id {
            Some(app_id) => result.found.push(Found {
                source_id: "tenoke",
                app_id,
                path: stats,
                format: Format::TenokeUserStats,
            }),
            None => result.unresolved.push(Unresolved {
                source_id: "tenoke",
                path: stats,
                reason: "app id not found",
            }),
        }
    }
    for dir in &settings {
        for (source_id, root) in save_roots(dir, folders) {
            result.found.extend(goldberg_saves(&root, source_id));
        }
    }
    let markers = scan_markers(&game.path, &bases, folders, app_id);
    result.found.extend(markers.found);
    result.unresolved.extend(markers.unresolved);
}

fn save_roots(settings: &Path, folders: &KnownFolders) -> Vec<(&'static str, PathBuf)> {
    let mut roots = Vec::new();
    let dll_dir = settings.parent().unwrap_or(settings);
    if let Some(ini) = read_text(&settings.join("configs.user.ini")).map(|t| Ini::parse(&t))
        && let Some(saves) = ini.section("user::saves")
    {
        if let Some(local) = saves
            .get("local_save_path")
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            let local = Path::new(local);
            let root = if local.is_absolute() {
                local.to_path_buf()
            } else {
                dll_dir.join(local)
            };
            roots.push(("gse_local", root));
        }
        if let Some(name) = saves
            .get("saves_folder_name")
            .map(str::trim)
            .filter(|name| is_plain_folder_name(name))
            && let Some(roaming) = folders.resolve(Base::RoamingAppData)
        {
            roots.push(("gse_custom", roaming.join(name)));
        }
    }
    roots.push(("gse_settings", settings.to_path_buf()));
    roots
}

fn is_plain_folder_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains([':', '\\', '/'])
        && !name.contains("..")
        && !DEFAULT_SAVE_FOLDERS
            .iter()
            .any(|default| default.eq_ignore_ascii_case(name))
}

fn goldberg_saves(root: &Path, source_id: &'static str) -> Vec<Found> {
    subdirs(root)
        .into_iter()
        .filter_map(|dir| {
            let name = dir.file_name()?.to_str()?;
            if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let app_id: u32 = name.parse().ok()?;
            let file = dir.join("achievements.json");
            file.is_file().then_some(Found {
                source_id,
                app_id,
                path: file,
                format: Format::GoldbergJson,
            })
        })
        .collect()
}

fn tenoke_app_id(base: &Path) -> Option<u32> {
    let ini = Ini::parse(&read_text(&base.join("tenoke.ini"))?);
    leading_digits(ini.section("TENOKE")?.get("id")?)
}

fn steam_appid_txt(dir: &Path) -> Option<u32> {
    leading_digits(&read_text(&dir.join("steam_appid.txt"))?)
}

pub(crate) fn read_text(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(text::decode(&bytes))
}

pub(crate) fn leading_digits(raw: &str) -> Option<u32> {
    let trimmed = raw.trim_start();
    let end = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    trimmed[..end].parse().ok().filter(|&id: &u32| id > 0)
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

    fn game(p: &Path) -> GameDir {
        GameDir {
            path: p.to_path_buf(),
            app_id_hint: None,
            from_steam_manifest: false,
        }
    }

    #[test]
    fn tenoke_with_tenoke_ini_resolves_app_id() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("Elden");
        touch(&g, "tenoke.ini", "[TENOKE]\nid = 1245620 # ELDEN RING\n");
        let stats = touch(&g, "SteamData/user_stats.ini", "[ACHIEVEMENTS]\n");
        let d = discover_game_dirs(&[game(&g)], &KnownFolders::under(tmp.path()));
        assert_eq!(
            d.found,
            vec![Found {
                source_id: "tenoke",
                app_id: 1245620,
                path: stats,
                format: Format::TenokeUserStats
            }]
        );
        assert!(d.unresolved.is_empty());
    }

    #[test]
    fn manifest_hint_wins_over_files() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("G");
        touch(&g, "steam_appid.txt", "999\n");
        touch(&g, "SteamData/user_stats.ini", "[ACHIEVEMENTS]\n");
        let mut gd = game(&g);
        gd.app_id_hint = Some(480);
        let d = discover_game_dirs(&[gd], &KnownFolders::under(tmp.path()));
        assert_eq!(d.found[0].app_id, 480);
    }

    #[test]
    fn steam_appid_txt_inside_steam_settings_resolves() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("G");
        touch(&g, "steam_settings/steam_appid.txt", "570\r\n");
        touch(&g, "SteamData/user_stats.ini", "[ACHIEVEMENTS]\n");
        let d = discover_game_dirs(&[game(&g)], &KnownFolders::under(tmp.path()));
        assert_eq!(d.found[0].app_id, 570);
    }

    #[test]
    fn tenoke_without_any_app_id_is_unresolved() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("G");
        let stats = touch(&g, "SteamData/user_stats.ini", "[ACHIEVEMENTS]\n");
        let d = discover_game_dirs(&[game(&g)], &KnownFolders::under(tmp.path()));
        assert!(d.found.is_empty());
        assert_eq!(
            d.unresolved,
            vec![Unresolved {
                source_id: "tenoke",
                path: stats,
                reason: "app id not found"
            }]
        );
    }

    #[test]
    fn goldberg_local_save_path_is_relative_to_dll_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("G");
        touch(
            &g,
            "Binaries/Win64/steam_settings/configs.user.ini",
            "[user::saves]\nlocal_save_path=./saves\n",
        );
        let save = touch(&g, "Binaries/Win64/saves/480/achievements.json", "{}");
        let d = discover_game_dirs(&[game(&g)], &KnownFolders::under(tmp.path()));
        assert_eq!(
            d.found,
            vec![Found {
                source_id: "gse_local",
                app_id: 480,
                path: save,
                format: Format::GoldbergJson
            }]
        );
    }

    #[test]
    fn goldberg_custom_saves_folder_name_lives_under_roaming() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("G");
        touch(
            &g,
            "steam_settings/configs.user.ini",
            "[user::saves]\nsaves_folder_name=My Saves\n",
        );
        let save = touch(
            tmp.path(),
            "RoamingAppData/My Saves/570/achievements.json",
            "{}",
        );
        let d = discover_game_dirs(&[game(&g)], &KnownFolders::under(tmp.path()));
        assert_eq!(
            d.found,
            vec![Found {
                source_id: "gse_custom",
                app_id: 570,
                path: save,
                format: Format::GoldbergJson
            }]
        );
    }

    #[test]
    fn goldberg_schema_file_is_never_state() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("G");
        touch(&g, "steam_settings/achievements.json", "[{\"name\":\"A\"}]");
        let save = touch(&g, "steam_settings/480/achievements.json", "{}");
        let d = discover_game_dirs(&[game(&g)], &KnownFolders::under(tmp.path()));
        assert_eq!(
            d.found,
            vec![Found {
                source_id: "gse_settings",
                app_id: 480,
                path: save,
                format: Format::GoldbergJson
            }]
        );
    }

    #[test]
    fn custom_saves_root_is_skipped_without_roaming() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("G");
        touch(
            &g,
            "steam_settings/configs.user.ini",
            "[user::saves]\nsaves_folder_name=My Saves\n",
        );
        touch(
            tmp.path(),
            "RoamingAppData/My Saves/570/achievements.json",
            "{}",
        );
        let folders = KnownFolders {
            roaming_app_data: None,
            ..KnownFolders::under(tmp.path())
        };
        assert!(discover_game_dirs(&[game(&g)], &folders).found.is_empty());
    }
}
