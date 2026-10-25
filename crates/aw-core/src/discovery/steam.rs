use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::discovery::Found;
use crate::parsers::vdf::{self, KvValue};
use crate::parsers::{Format, steamstats, text};

const STEAM_INSTALLED_FLAG: u64 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApp {
    pub app_id: u32,
    pub name: String,
    pub install_dir: PathBuf,
}

pub fn pick_steam_path(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates
        .into_iter()
        .find(|candidate| candidate.join("steam.exe").is_file())
}

pub fn steam_install_path() -> Option<PathBuf> {
    pick_steam_path(registry_candidates())
}

pub fn library_folders(steam: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![steam.to_path_buf()];
    let from_steamapps = read_library_vdf(&steam.join("steamapps").join("libraryfolders.vdf"));
    let listed = if from_steamapps.is_empty() {
        read_library_vdf(&steam.join("config").join("libraryfolders.vdf"))
    } else {
        from_steamapps
    };
    libraries.extend(listed.into_iter().filter(|path| path.is_dir()));
    dedupe(libraries)
}

pub fn installed_apps(library: &Path) -> Vec<InstalledApp> {
    let steamapps = library.join("steamapps");
    let Ok(entries) = std::fs::read_dir(&steamapps) else {
        return Vec::new();
    };
    let mut apps: Vec<InstalledApp> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_ascii_lowercase();
            let digits = name.strip_prefix("appmanifest_")?.strip_suffix(".acf")?;
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            read_manifest(&entry.path(), digits.parse().ok()?, &steamapps)
        })
        .collect();
    apps.sort_by_key(|app| app.app_id);
    apps
}

pub fn stats_files(steam: &Path) -> Vec<Found> {
    let dir = steam.join("appcache").join("stats");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut found: Vec<Found> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            let lower = name.to_ascii_lowercase();
            let rest = lower.strip_prefix("usergamestats_")?.strip_suffix(".bin")?;
            let (_, app) = rest.rsplit_once('_')?;
            let app_id: u32 = app.parse().ok()?;
            let path = entry.path();
            steamstats::schema_path(&path)
                .is_some_and(|schema| schema.is_file())
                .then_some(Found {
                    source_id: "steam",
                    app_id,
                    path,
                    format: Format::SteamStats,
                })
        })
        .collect();
    found.sort_by(|a, b| (a.app_id, &a.path).cmp(&(b.app_id, &b.path)));
    found
}

pub fn normalize_for_compare(path: &Path) -> String {
    let lowered = path.to_string_lossy().to_lowercase().replace('/', "\\");
    lowered.trim_end_matches('\\').to_owned()
}

fn read_manifest(path: &Path, app_id: u32, steamapps: &Path) -> Option<InstalledApp> {
    let kv = read_vdf(path)?;
    let state = kv.get("AppState")?;
    let declared: u32 = state.get("appid")?.as_str()?.trim().parse().ok()?;
    if declared != app_id {
        return None;
    }
    let flags: u64 = state.get("StateFlags")?.as_str()?.trim().parse().ok()?;
    if flags & STEAM_INSTALLED_FLAG == 0 {
        return None;
    }
    let install_dir = state.get("installdir")?.as_str()?.trim();
    if install_dir.is_empty()
        || install_dir.contains(['/', '\\'])
        || install_dir == "."
        || install_dir == ".."
    {
        return None;
    }
    let dir = steamapps.join("common").join(install_dir);
    if !dir.is_dir() {
        return None;
    }
    let name = state
        .get("name")
        .and_then(KvValue::as_str)
        .unwrap_or_default()
        .to_owned();
    Some(InstalledApp {
        app_id,
        name,
        install_dir: dir,
    })
}

pub fn parse_localconfig_playtime(text: &str) -> HashMap<u32, u64> {
    let Ok(root) = vdf::parse(text) else {
        return HashMap::new();
    };
    ["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"]
        .iter()
        .try_fold(&root, |node, key| node.get(key))
        .map(|apps| {
            apps.entries()
                .iter()
                .filter_map(|(id, app)| {
                    let minutes = app.get("Playtime")?.as_str()?.parse().ok()?;
                    Some((id.parse().ok()?, minutes))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn localconfig_files(steam: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(steam.join("userdata")) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path().join("config").join("localconfig.vdf"))
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

pub fn localconfig_playtimes(steam: &Path) -> HashMap<u32, u64> {
    let mut merged: HashMap<u32, u64> = HashMap::new();
    for file in localconfig_files(steam) {
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        for (app_id, minutes) in parse_localconfig_playtime(&text::decode(&bytes)) {
            let slot = merged.entry(app_id).or_insert(0);
            *slot = (*slot).max(minutes);
        }
    }
    merged
}

fn read_vdf(path: &Path) -> Option<KvValue> {
    let bytes = std::fs::read(path).ok()?;
    vdf::parse(&text::decode(&bytes)).ok()
}

fn read_library_vdf(path: &Path) -> Vec<PathBuf> {
    let Some(kv) = read_vdf(path) else {
        return Vec::new();
    };
    let Some(root) = kv.get("libraryfolders") else {
        return Vec::new();
    };
    root.entries()
        .iter()
        .filter_map(|(_, value)| match value {
            KvValue::Obj(_) => value.get("path").and_then(KvValue::as_str),
            KvValue::Str(s) => Path::new(s).is_absolute().then_some(s.as_str()),
        })
        .map(|raw| PathBuf::from(raw.replace('/', "\\")))
        .collect()
}

fn dedupe(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(normalize_for_compare(path)))
        .collect()
}

#[cfg(windows)]
fn registry_candidates() -> Vec<PathBuf> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use windows::core::w;

    [
        registry_string(
            HKEY_CURRENT_USER,
            w!("Software\\Valve\\Steam"),
            w!("SteamPath"),
        ),
        registry_string(
            HKEY_LOCAL_MACHINE,
            w!("Software\\WOW6432Node\\Valve\\Steam"),
            w!("InstallPath"),
        ),
    ]
    .into_iter()
    .flatten()
    .map(|raw| PathBuf::from(raw.replace('/', "\\")))
    .collect()
}

#[cfg(not(windows))]
fn registry_candidates() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(windows)]
fn registry_string(
    hkey: windows::Win32::System::Registry::HKEY,
    subkey: windows::core::PCWSTR,
    value: windows::core::PCWSTR,
) -> Option<String> {
    use windows::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};

    let mut size: u32 = 0;
    // SAFETY: valid constant strings, size query with no data buffer.
    let status = unsafe {
        RegGetValueW(
            hkey,
            subkey,
            value,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if status.is_err() || size < 2 {
        return None;
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
    // SAFETY: `buffer` holds `size` bytes and outlives the call.
    let status = unsafe {
        RegGetValueW(
            hkey,
            subkey,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return None;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..len]);
    (!text.trim().is_empty()).then_some(text)
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

    fn vdf_path(p: &Path) -> String {
        p.display().to_string().replace('\\', "\\\\")
    }

    fn localconfig(apps: &[(u32, u64)]) -> String {
        let body: String = apps
            .iter()
            .map(|(id, minutes)| {
                format!("\"{id}\" {{ \"Playtime\" \"{minutes}\" \"cloud\" {{ \"last_sync_state\" \"synchronized\" }} }}\n")
            })
            .collect();
        format!(
            "\"UserLocalConfigStore\" {{ \"Software\" {{ \"Valve\" {{ \"Steam\" {{ \"apps\" {{\n{body}}} }} }} }} }}\n"
        )
    }

    #[test]
    fn playtime_parsed_per_app() {
        let map = parse_localconfig_playtime(&localconfig(&[(413150, 28200), (1583320, 15090)]));
        assert_eq!(
            map,
            std::collections::HashMap::from([(413150, 28200), (1583320, 15090)])
        );
    }

    #[test]
    fn max_across_accounts() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            tmp.path(),
            "userdata/1/config/localconfig.vdf",
            &localconfig(&[(7, 100), (8, 5)]),
        );
        touch(
            tmp.path(),
            "userdata/2/config/localconfig.vdf",
            &localconfig(&[(7, 250)]),
        );
        let map = localconfig_playtimes(tmp.path());
        assert_eq!(map.get(&7), Some(&250));
        assert_eq!(map.get(&8), Some(&5));
    }

    #[test]
    fn missing_files_yield_empty_map() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(localconfig_playtimes(tmp.path()).is_empty());
    }

    #[test]
    fn garbage_vdf_yields_empty_map() {
        assert!(parse_localconfig_playtime("{{{").is_empty());
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "userdata/1/config/localconfig.vdf", "\"x\" {");
        assert!(localconfig_playtimes(tmp.path()).is_empty());
    }

    fn manifest(app_id: u32, flags: u32, installdir: &str) -> String {
        format!(
            "\"AppState\"\n{{\n \"appid\" \"{app_id}\"\n \"name\" \"Game {app_id}\"\n \"StateFlags\" \"{flags}\"\n \"installdir\" \"{installdir}\"\n}}\n"
        )
    }

    #[test]
    fn pick_steam_path_requires_steam_exe() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("SomeGame");
        std::fs::create_dir_all(&game).unwrap();
        touch(tmp.path(), "Steam/steam.exe", "");
        let steam = tmp.path().join("Steam");
        assert_eq!(pick_steam_path([game, steam.clone()]), Some(steam));
        assert_eq!(pick_steam_path([tmp.path().join("nope")]), None);
    }

    #[test]
    fn library_folders_reads_modern_vdf_skips_missing_and_dedupes() {
        let tmp = tempfile::tempdir().unwrap();
        let steam = tmp.path().join("Steam");
        let lib2 = tmp.path().join("Lib2");
        std::fs::create_dir_all(&lib2).unwrap();
        let same_steam = steam
            .display()
            .to_string()
            .to_uppercase()
            .replace('\\', "/");
        let body = format!(
            "\"libraryfolders\"\n{{\n \"0\" {{ \"path\" \"{}\" }}\n \"1\" {{ \"path\" \"{}\" }}\n \"2\" {{ \"path\" \"{}\" }}\n}}\n",
            same_steam,
            vdf_path(&lib2),
            vdf_path(&tmp.path().join("RemovedDrive")),
        );
        touch(&steam, "steamapps/libraryfolders.vdf", &body);
        assert_eq!(library_folders(&steam), vec![steam.clone(), lib2]);
    }

    #[test]
    fn library_folders_falls_back_to_config_vdf() {
        let tmp = tempfile::tempdir().unwrap();
        let steam = tmp.path().join("Steam");
        let lib2 = tmp.path().join("Lib2");
        std::fs::create_dir_all(&lib2).unwrap();
        touch(
            &steam,
            "config/libraryfolders.vdf",
            &format!(
                "\"libraryfolders\" {{ \"1\" {{ \"path\" \"{}\" }} }}",
                vdf_path(&lib2)
            ),
        );
        assert_eq!(library_folders(&steam), vec![steam.clone(), lib2]);
    }

    #[test]
    fn library_folders_reads_legacy_values() {
        let tmp = tempfile::tempdir().unwrap();
        let steam = tmp.path().join("Steam");
        let lib2 = tmp.path().join("Lib2");
        std::fs::create_dir_all(&lib2).unwrap();
        touch(
            &steam,
            "steamapps/libraryfolders.vdf",
            &format!(
                "\"LibraryFolders\" {{ \"TimeNextStatsReport\" \"123\" \"1\" \"{}\" }}",
                vdf_path(&lib2)
            ),
        );
        assert_eq!(library_folders(&steam), vec![steam.clone(), lib2]);
    }

    #[cfg(windows)]
    #[test]
    fn steam_install_path_is_none_or_has_steam_exe() {
        if let Some(p) = steam_install_path() {
            assert!(p.join("steam.exe").is_file());
        }
    }

    #[test]
    fn installed_apps_reads_valid_manifests_only() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = tmp.path();
        touch(
            lib,
            "steamapps/appmanifest_480.acf",
            &manifest(480, 4, "Spacewar"),
        );
        touch(lib, "steamapps/common/Spacewar/game.exe", "");
        touch(
            lib,
            "steamapps/APPMANIFEST_570.ACF",
            &manifest(570, 6, "dota 2 beta"),
        );
        touch(lib, "steamapps/common/dota 2 beta/game.exe", "");
        touch(
            lib,
            "steamapps/appmanifest_10.acf",
            &manifest(10, 2, "Half-Life"),
        );
        touch(lib, "steamapps/common/Half-Life/hl.exe", "");
        touch(
            lib,
            "steamapps/appmanifest_20.acf",
            &manifest(21, 4, "Mismatch"),
        );
        touch(lib, "steamapps/common/Mismatch/x.exe", "");
        touch(
            lib,
            "steamapps/appmanifest_30.acf",
            &manifest(30, 4, "..\\\\escape"),
        );
        touch(
            lib,
            "steamapps/appmanifest_40.acf",
            &manifest(40, 4, "NotThere"),
        );
        let apps = installed_apps(lib);
        let got: Vec<(u32, String)> = apps.iter().map(|a| (a.app_id, a.name.clone())).collect();
        assert_eq!(
            got,
            vec![(480, "Game 480".into()), (570, "Game 570".into())]
        );
        assert_eq!(
            apps[0].install_dir,
            lib.join("steamapps").join("common").join("Spacewar")
        );
    }
}
