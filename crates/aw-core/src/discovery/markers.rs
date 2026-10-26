use std::path::{Path, PathBuf};

use crate::discovery::Found;
use crate::discovery::folders::{Base, KnownFolders};
use crate::discovery::gamedirs::subdirs;
use crate::discovery::gamediscovery::{Unresolved, leading_digits, read_text};
use crate::parsers::Format;
use crate::parsers::ini::{Ini, Section};
use crate::platform::{ExternalId, Platform, valid_external_id};

const ALI213_MARKERS: [&str; 3] = ["ALI213.ini", "valve.ini", "SteamConfig.ini"];
const GAME_SETTINGS_MARKERS: [(&str, &str); 3] = [
    ("hlm.ini", "hoodlum"),
    ("ds.ini", "darksiders"),
    ("steam_api.ini", "skidrow_game"),
];
const ALI213_FILE: &str = "Achievements.Bin";
const UNIVERSELAN_MARKER: &str = "UniverseLAN.ini";
const GOG_INFO_PREFIX: &str = "goggame-";
const GOG_INFO_SUFFIX: &str = ".info";
const GOG_INFO_LEVELS: usize = 4;

#[derive(Debug, Default)]
pub struct MarkerResult {
    pub found: Vec<Found>,
    pub unresolved: Vec<Unresolved>,
}

impl MarkerResult {
    fn add(&mut self, source_id: &'static str, app_id: Option<u32>, path: PathBuf, format: Format) {
        match app_id {
            Some(app_id) => self.found.push(Found {
                source_id,
                app_id,
                external: None,
                path,
                format,
            }),
            None => self.unresolved.push(Unresolved {
                source_id,
                path,
                reason: "app id not found",
            }),
        }
    }

    fn add_external(
        &mut self,
        source_id: &'static str,
        external: Option<ExternalId>,
        path: PathBuf,
        format: Format,
    ) {
        match external {
            Some(external) => self
                .found
                .push(Found::external(source_id, external, path, format)),
            None => self.unresolved.push(Unresolved {
                source_id,
                path,
                reason: "app id not found",
            }),
        }
    }
}

pub fn scan_markers(
    root: &Path,
    bases: &[PathBuf],
    folders: &KnownFolders,
    fallback_app_id: Option<u32>,
) -> MarkerResult {
    let mut result = MarkerResult::default();
    let documents = folders.resolve(Base::Documents);
    for base in bases {
        ali213(root, base, documents, fallback_app_id, &mut result);
        game_settings(root, base, documents, fallback_app_id, &mut result);
        catherine(root, base, fallback_app_id, &mut result);
        threedm_game(base, fallback_app_id, &mut result);
        universelan(root, base, &mut result);
    }
    result
}

fn ini_at(path: &Path) -> Option<Ini> {
    path.is_file()
        .then(|| read_text(path))
        .flatten()
        .map(|text| Ini::parse(&text))
}

fn non_empty<'a>(section: &'a Section, key: &str) -> Option<&'a str> {
    section.get(key).map(str::trim).filter(|v| !v.is_empty())
}

fn find_up(start: &Path, root: &Path, rel: &[&str]) -> Option<PathBuf> {
    start
        .ancestors()
        .take_while(|dir| dir.starts_with(root))
        .map(|dir| rel.iter().fold(dir.to_path_buf(), |p, s| p.join(s)))
        .find(|candidate| candidate.is_file())
}

fn ali213(
    root: &Path,
    base: &Path,
    documents: Option<&Path>,
    fallback: Option<u32>,
    result: &mut MarkerResult,
) {
    for marker in ALI213_MARKERS {
        let Some(ini) = ini_at(&base.join(marker)) else {
            continue;
        };
        let Some(settings) = ini.section("Settings") else {
            continue;
        };
        let app_id = settings.get("AppID").and_then(leading_digits).or(fallback);
        let player = non_empty(settings, "PlayerName");
        let save_type = settings.get("SaveType").map(str::trim);
        let mut files = Vec::new();
        match (save_type, player) {
            (Some("1"), Some(player)) => {
                if let (Some(docs), Some(app)) = (documents, app_id) {
                    let path = docs
                        .join("VALVE")
                        .join(app.to_string())
                        .join(player)
                        .join("Stats")
                        .join(ALI213_FILE);
                    if path.is_file() {
                        files.push(path);
                    }
                }
            }
            (Some("0" | ""), Some(player)) => {
                files.extend(find_up(
                    base,
                    root,
                    &["Profile", player, "Stats", ALI213_FILE],
                ));
            }
            (None, _) => files.extend(find_up(base, root, &["Profile", "Stats", ALI213_FILE])),
            _ => {}
        }
        for profile in subdirs(&base.join("Profile")) {
            let path = profile.join("Stats").join(ALI213_FILE);
            if path.is_file() {
                files.push(path);
            }
        }
        for path in files {
            result.add("ali213", app_id, path, Format::GenericIni);
        }
    }
}

fn game_settings(
    root: &Path,
    base: &Path,
    documents: Option<&Path>,
    fallback: Option<u32>,
    result: &mut MarkerResult,
) {
    for (marker, source_id) in GAME_SETTINGS_MARKERS {
        let Some(ini) = ini_at(&base.join(marker)) else {
            continue;
        };
        let Some(settings) = ini.section("GameSettings") else {
            continue;
        };
        let app_id = settings.get("AppId").and_then(leading_digits).or(fallback);
        match non_empty(settings, "UserDataFolder") {
            Some(".") => {
                if let Some(path) = find_up(base, root, &["SteamEmu", "UserStats", "achiev.ini"])
                    .or_else(|| find_up(base, root, &["SteamEmu", "stats.ini"]))
                {
                    result.add(source_id, app_id, path, Format::AchievIni);
                } else if marker == "hlm.ini"
                    && let Some(path) =
                        find_up(base, root, &["Profile", "VALVE", "Stats", ALI213_FILE])
                {
                    result.add(source_id, app_id, path, Format::GenericIni);
                }
            }
            Some(mode) if mode.eq_ignore_ascii_case("mydocs") => {
                let (Some(docs), Some(user), Some(app)) =
                    (documents, non_empty(settings, "UserName"), app_id)
                else {
                    continue;
                };
                let emu = docs.join(user).join(app.to_string()).join("SteamEmu");
                let candidates = [
                    emu.join("UserStats").join("achiev.ini"),
                    emu.join("stats.ini"),
                ];
                if let Some(path) = candidates.into_iter().find(|p| p.is_file()) {
                    result.add(source_id, Some(app), path, Format::AchievIni);
                }
            }
            _ => {}
        }
    }
}

fn catherine(root: &Path, base: &Path, fallback: Option<u32>, result: &mut MarkerResult) {
    let Some(ini) = ini_at(&base.join("steam_api.ini")) else {
        return;
    };
    if ini.section("GameSettings").is_some() {
        return;
    }
    let Some(settings) = ini.section("Settings") else {
        return;
    };
    let Some(steam_id) = non_empty(settings, "SteamID") else {
        return;
    };
    let app_id = settings.get("AppId").and_then(leading_digits).or(fallback);
    if let Some(path) = find_up(base, root, &["SteamProfile", steam_id, "Achievements.ini"]) {
        result.add("catherine", app_id, path, Format::GenericIni);
    }
}

fn threedm_game(base: &Path, fallback: Option<u32>, result: &mut MarkerResult) {
    for profile in subdirs(&base.join("3DMGAME")) {
        let path = profile.join("stats").join("achievements.ini");
        if path.is_file() {
            result.add("3dm_game", fallback, path, Format::ReloadedIni);
        }
    }
}

fn universelan(root: &Path, base: &Path, result: &mut MarkerResult) {
    if !base.join(UNIVERSELAN_MARKER).is_file() {
        return;
    }
    let path = base.join("UniverseLANData").join("Achievements.ini");
    if !path.is_file() {
        return;
    }
    let external = base
        .ancestors()
        .take(GOG_INFO_LEVELS)
        .take_while(|dir| dir.starts_with(root))
        .find_map(gog_root_id)
        .map(|id| ExternalId {
            platform: Platform::Gog,
            id,
        });
    result.add_external("universelan_game", external, path, Format::GenericIni);
}

fn gog_root_id(dir: &Path) -> Option<String> {
    let mut infos: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    let lower = name.to_ascii_lowercase();
                    lower.starts_with(GOG_INFO_PREFIX) && lower.ends_with(GOG_INFO_SUFFIX)
                })
        })
        .collect();
    infos.sort();
    infos.iter().find_map(|path| {
        let text = read_text(path)?;
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        let id = match json.get("rootGameId")? {
            serde_json::Value::String(id) => id.trim().to_owned(),
            serde_json::Value::Number(id) => id.to_string(),
            _ => return None,
        };
        valid_external_id(&id).then_some(id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::gamefolder::base_dirs;

    fn touch(root: &Path, rel: &str, body: &str) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
        p
    }

    fn scan(tmp: &Path, game: &Path, fallback: Option<u32>) -> MarkerResult {
        scan_markers(game, &base_dirs(game), &KnownFolders::under(tmp), fallback)
    }

    fn ids(result: &MarkerResult) -> Vec<(&'static str, u32, Format)> {
        result
            .found
            .iter()
            .map(|f| (f.source_id, f.app_id, f.format))
            .collect()
    }

    #[test]
    fn ali213_profile_stats_found() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("Game");
        touch(
            &g,
            "bin/ALI213.ini",
            "[Settings]\nAppID=500\nPlayerName=VALVE\nSaveType=0\n",
        );
        touch(
            &g,
            "Profile/VALVE/Stats/Achievements.Bin",
            "[A]\nHaveAchieved=1\n",
        );
        let r = scan(tmp.path(), &g, None);
        assert!(ids(&r).contains(&("ali213", 500, Format::GenericIni)));
    }

    #[test]
    fn ali213_documents_save_type() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("Game");
        touch(
            &g,
            "valve.ini",
            "[Settings]\nAppID=501\nPlayerName=Me\nSaveType=1\n",
        );
        touch(
            tmp.path(),
            "Documents/VALVE/501/Me/Stats/Achievements.Bin",
            "[A]\nHaveAchieved=1\n",
        );
        assert_eq!(
            ids(&scan(tmp.path(), &g, None)),
            vec![("ali213", 501, Format::GenericIni)]
        );
    }

    #[test]
    fn hoodlum_userdatafolder_dot() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("Game");
        touch(
            &g,
            "hlm.ini",
            "[GameSettings]\nAppId=600\nUserDataFolder=.\n",
        );
        touch(&g, "SteamEmu/UserStats/achiev.ini", "[Achievements]\nA=1\n");
        assert_eq!(
            ids(&scan(tmp.path(), &g, None)),
            vec![("hoodlum", 600, Format::AchievIni)]
        );
    }

    #[test]
    fn hoodlum_mydocs() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("Game");
        touch(
            &g,
            "ds.ini",
            "[GameSettings]\nAppId=601\nUserDataFolder=mydocs\nUserName=Player\n",
        );
        touch(
            tmp.path(),
            "Documents/Player/601/SteamEmu/stats.ini",
            "[Achievements]\nA=1\n",
        );
        assert_eq!(
            ids(&scan(tmp.path(), &g, None)),
            vec![("darksiders", 601, Format::AchievIni)]
        );
    }

    #[test]
    fn threedmgame_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("Game");
        touch(
            &g,
            "3DMGAME/Player/stats/achievements.ini",
            "[State]\nA=0101\n",
        );
        assert_eq!(
            ids(&scan(tmp.path(), &g, Some(700))),
            vec![("3dm_game", 700, Format::ReloadedIni)]
        );
        let unresolved = scan(tmp.path(), &g, None);
        assert_eq!(unresolved.unresolved.len(), 1);
    }

    #[test]
    fn catherine_steamprofile() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("Game");
        touch(&g, "steam_api.ini", "[Settings]\nAppId=800\nSteamID=7656\n");
        touch(
            &g,
            "SteamProfile/7656/Achievements.ini",
            "[A]\nAchieved=1\n",
        );
        assert_eq!(
            ids(&scan(tmp.path(), &g, None)),
            vec![("catherine", 800, Format::GenericIni)]
        );
    }

    #[test]
    fn markers_without_files_find_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("Game");
        touch(&g, "hlm.ini", "[GameSettings]\nAppId=1\nUserDataFolder=.\n");
        touch(&g, "steam_api.ini", "[Settings]\nAppId=2\n");
        let r = scan(tmp.path(), &g, None);
        assert!(r.found.is_empty() && r.unresolved.is_empty());
    }

    #[test]
    fn universelan_marker_uses_goggame_info() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("Game");
        touch(
            &g,
            "goggame-1207658924.info",
            r#"{"gameId":"1207658924","rootGameId":"1207658924"}"#,
        );
        touch(
            &g,
            "bin/UniverseLAN.ini",
            "[Settings]
",
        );
        let file = touch(
            &g,
            "bin/UniverseLANData/Achievements.ini",
            "[A]
Unlocked=true
",
        );
        let r = scan(tmp.path(), &g, None);
        assert_eq!(r.found.len(), 1);
        assert_eq!(r.found[0].source_id, "universelan_game");
        assert_eq!(r.found[0].path, file);
        let external = r.found[0].external.as_ref().unwrap();
        assert_eq!(external.platform, crate::platform::Platform::Gog);
        assert_eq!(external.id, "1207658924");
    }

    #[test]
    fn universelan_marker_without_info_is_unresolved() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path().join("Game");
        touch(
            &g,
            "UniverseLAN.ini",
            "[Settings]
",
        );
        touch(
            &g,
            "UniverseLANData/Achievements.ini",
            "[A]
Unlocked=true
",
        );
        let r = scan(tmp.path(), &g, None);
        assert!(r.found.is_empty());
        assert_eq!(r.unresolved.len(), 1);
    }
}
