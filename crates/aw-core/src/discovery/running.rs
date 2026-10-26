use std::path::{Path, PathBuf};

use crate::discovery::gamediscovery::{leading_digits, read_text};
use crate::discovery::steam::normalize_for_compare;
use crate::parsers::ini::Ini;

const SEARCH_LEVELS: usize = 3;
const MARKER_FILES: [&str; 12] = [
    "steam_api.dll",
    "steam_api64.dll",
    "steam_appid.txt",
    "OnlineFix.ini",
    "steam_emu.ini",
    "tenoke.ini",
    "ALI213.ini",
    "SteamConfig.ini",
    "hlm.ini",
    "ds.ini",
    "steam_api.ini",
    "valve.ini",
];
const SYSTEM_DIRS: [&str; 5] = [
    "\\windows\\",
    "\\windowsapps\\",
    "\\microsoft\\",
    "\\nvidia corporation\\",
    "\\common files\\",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub exe: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownGame {
    pub dir: PathBuf,
    pub app_id: u32,
}

pub fn is_system_exe(exe: &Path) -> bool {
    let lower = normalize_for_compare(exe);
    SYSTEM_DIRS.iter().any(|dir| lower.contains(dir))
}

pub fn match_known(exe: &Path, games: &[KnownGame]) -> Option<u32> {
    let exe = normalize_for_compare(exe);
    games
        .iter()
        .filter_map(|game| {
            let dir = normalize_for_compare(&game.dir);
            (exe.starts_with(&dir) && exe[dir.len()..].starts_with('\\'))
                .then_some((dir.len(), game.app_id))
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, app_id)| app_id)
}

pub fn app_id_in(dir: &Path) -> Option<u32> {
    let ini_value = |file: &str, section: &str, key: &str| {
        read_text(&dir.join(file))
            .map(|t| Ini::parse(&t))
            .and_then(|ini| {
                ini.section(section)
                    .and_then(|s| s.get(key))
                    .and_then(leading_digits)
            })
    };
    read_text(&dir.join("steam_appid.txt"))
        .as_deref()
        .and_then(leading_digits)
        .or_else(|| {
            read_text(&dir.join("steam_settings").join("steam_appid.txt"))
                .as_deref()
                .and_then(leading_digits)
        })
        .or_else(|| ini_value("OnlineFix.ini", "Main", "RealAppId"))
        .or_else(|| ini_value("steam_emu.ini", "Settings", "AppId"))
        .or_else(|| ini_value("tenoke.ini", "TENOKE", "id"))
        .or_else(|| ini_value("ALI213.ini", "Settings", "AppID"))
        .or_else(|| ini_value("SteamConfig.ini", "Settings", "AppID"))
        .or_else(|| ini_value("valve.ini", "Settings", "AppID"))
        .or_else(|| ini_value("hlm.ini", "GameSettings", "AppId"))
        .or_else(|| ini_value("ds.ini", "GameSettings", "AppId"))
        .or_else(|| ini_value("steam_api.ini", "GameSettings", "AppId"))
        .or_else(|| ini_value("steam_api.ini", "Settings", "AppId"))
}

pub fn game_root_for_exe(exe: &Path) -> Option<(PathBuf, Option<u32>)> {
    let dir = exe.parent()?;
    let root = dir
        .ancestors()
        .take(SEARCH_LEVELS + 1)
        .filter(|d| d.parent().is_some())
        .find(|d| {
            MARKER_FILES.iter().any(|m| d.join(m).is_file()) || d.join("steam_settings").is_dir()
        })?;
    let app_id = root
        .ancestors()
        .take(SEARCH_LEVELS + 1)
        .take_while(|d| d.parent().is_some())
        .find_map(app_id_in)
        .or_else(|| dir.ancestors().take(SEARCH_LEVELS + 1).find_map(app_id_in));
    Some((root.to_path_buf(), app_id))
}

#[cfg(windows)]
pub fn list_processes() -> Vec<ProcessInfo> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::core::PWSTR;

    let own = std::process::id();
    // SAFETY: snapshot of all processes, handle closed below.
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return Vec::new();
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut pids = Vec::new();
    // SAFETY: `entry.dwSize` is set and the snapshot handle is valid.
    let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while ok {
        if entry.th32ProcessID != 0 && entry.th32ProcessID != own {
            pids.push(entry.th32ProcessID);
        }
        // SAFETY: same snapshot and entry as above.
        ok = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    // SAFETY: the snapshot handle is owned here and closed once.
    let _ = unsafe { CloseHandle(snapshot) };
    pids.into_iter()
        .filter_map(|pid| {
            // SAFETY: query-only access to a process id; handle closed below.
            let handle =
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
            let mut buffer = [0u16; 1024];
            let mut len = buffer.len() as u32;
            // SAFETY: buffer and length describe valid writable memory.
            let queried = unsafe {
                QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_WIN32,
                    PWSTR(buffer.as_mut_ptr()),
                    &mut len,
                )
            };
            // SAFETY: handle came from OpenProcess and is closed once.
            let _ = unsafe { CloseHandle(handle) };
            queried.ok()?;
            let exe = PathBuf::from(String::from_utf16_lossy(&buffer[..len as usize]));
            Some(ProcessInfo { pid, exe })
        })
        .collect()
}

#[cfg(not(windows))]
pub fn list_processes() -> Vec<ProcessInfo> {
    Vec::new()
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
    fn match_exe_to_game_dir() {
        let games = vec![
            KnownGame {
                dir: PathBuf::from(r"C:\Games\A"),
                app_id: 1,
            },
            KnownGame {
                dir: PathBuf::from(r"C:\Games\A\Mods\B"),
                app_id: 2,
            },
        ];
        assert_eq!(
            match_known(Path::new(r"c:\games\a\bin\a.exe"), &games),
            Some(1)
        );
        assert_eq!(
            match_known(Path::new(r"C:\Games\A\Mods\B\b.exe"), &games),
            Some(2)
        );
        assert_eq!(match_known(Path::new(r"C:\Games\AB\x.exe"), &games), None);
    }

    #[test]
    fn running_exe_adds_game_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("ELDEN RING").join("Game");
        touch(
            &game,
            "OnlineFix.ini",
            "[Main]\nRealAppId=1245620\nFakeAppId=480\n",
        );
        let exe = touch(&game, "eldenring.exe", "");
        assert_eq!(
            game_root_for_exe(&exe),
            Some((game.clone(), Some(1_245_620)))
        );
        let nested = touch(&game, "Binaries/Win64/game.exe", "");
        assert_eq!(game_root_for_exe(&nested), Some((game, Some(1_245_620))));
    }

    #[test]
    fn exe_without_markers_is_not_a_game() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = touch(tmp.path(), "Tools/app/app.exe", "");
        assert_eq!(game_root_for_exe(&exe), None);
    }

    #[test]
    fn app_id_from_each_marker() {
        let tmp = tempfile::tempdir().unwrap();
        for (i, (file, body)) in [
            ("steam_appid.txt", "10\n"),
            ("steam_emu.ini", "[Settings]\nAppId=11\n"),
            ("tenoke.ini", "[TENOKE]\nid = 12\n"),
            ("hlm.ini", "[GameSettings]\nAppId=13\n"),
        ]
        .iter()
        .enumerate()
        {
            let dir = tmp.path().join(format!("g{i}"));
            touch(&dir, file, body);
            assert_eq!(app_id_in(&dir), Some(10 + i as u32), "{file}");
        }
    }

    #[test]
    fn system_paths_are_ignored() {
        assert!(is_system_exe(Path::new(r"C:\Windows\System32\svchost.exe")));
        assert!(is_system_exe(Path::new(
            r"C:\Program Files\WindowsApps\x\y.exe"
        )));
        assert!(!is_system_exe(Path::new(
            r"C:\Jogos\ELDEN RING\Game\eldenring.exe"
        )));
    }

    #[cfg(windows)]
    #[test]
    fn lists_running_processes() {
        let processes = list_processes();
        assert!(processes.iter().any(|p| p.exe.extension().is_some()));
    }
}
