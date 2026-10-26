use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::discovery::Found;
use crate::parsers::Format;
use crate::platform::{ExternalId, Platform, valid_external_id};

pub const LUMAPLAY_ROOT: &str = r"SOFTWARE\LumaPlay";
const HKCU_PREFIX: &str = r"HKCU\";
const ACHIEVEMENTS_KEY: &str = "Achievements";

pub trait Registry: Send + Sync {
    fn subkeys(&self, path: &str) -> Vec<String>;
    fn values(&self, path: &str) -> Option<Vec<(String, u64)>>;
    fn last_write(&self, path: &str) -> Option<i64>;
}

pub struct WindowsRegistry;

#[derive(Debug, Clone, Default)]
pub struct MemoryRegistry {
    pub keys: HashMap<String, Vec<String>>,
    pub values: HashMap<String, Vec<(String, u64)>>,
    pub written: HashMap<String, i64>,
}

impl Registry for MemoryRegistry {
    fn subkeys(&self, path: &str) -> Vec<String> {
        self.keys.get(path).cloned().unwrap_or_default()
    }

    fn values(&self, path: &str) -> Option<Vec<(String, u64)>> {
        self.values.get(path).cloned()
    }

    fn last_write(&self, path: &str) -> Option<i64> {
        self.written.get(path).copied()
    }
}

pub fn discover(registry: &dyn Registry) -> Vec<Found> {
    let mut found = Vec::new();
    for user in registry.subkeys(LUMAPLAY_ROOT) {
        for id in registry.subkeys(&format!(r"{LUMAPLAY_ROOT}\{user}")) {
            if !valid_external_id(&id) {
                continue;
            }
            let key = format!(r"{LUMAPLAY_ROOT}\{user}\{id}\{ACHIEVEMENTS_KEY}");
            if registry.values(&key).is_none() {
                continue;
            }
            found.push(Found::external(
                "lumaplay",
                ExternalId {
                    platform: Platform::Ubisoft,
                    id,
                },
                PathBuf::from(format!("{HKCU_PREFIX}{key}")),
                Format::LumaPlay,
            ));
        }
    }
    found
}

pub fn is_registry_path(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|text| text.starts_with(HKCU_PREFIX))
}

pub fn written_at(registry: &dyn Registry, path: &Path) -> Option<i64> {
    registry.last_write(path.to_str()?.strip_prefix(HKCU_PREFIX)?)
}

pub fn snapshot(registry: &dyn Registry, path: &Path) -> io::Result<Vec<u8>> {
    let key = path
        .to_str()
        .and_then(|text| text.strip_prefix(HKCU_PREFIX))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a registry key"))?;
    let mut values = registry
        .values(key)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "registry key not found"))?;
    values.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(values
        .into_iter()
        .map(|(name, value)| format!("{name}={value}\n"))
        .collect::<String>()
        .into_bytes())
}

#[cfg(windows)]
mod win {
    use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, FILETIME};
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD, REG_QWORD, RegCloseKey, RegEnumKeyExW,
        RegEnumValueW, RegOpenKeyExW, RegQueryInfoKeyW,
    };
    use windows::core::{HSTRING, PWSTR};

    use super::{Registry, WindowsRegistry};

    const MAX_KEY_NAME: usize = 256;
    const MAX_VALUE_NAME: usize = 16_384;
    const FILETIME_TICKS_PER_SECOND: u64 = 10_000_000;
    const FILETIME_UNIX_OFFSET: i64 = 11_644_473_600;

    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: the handle came from a successful RegOpenKeyExW and is closed only here.
            let _ = unsafe { RegCloseKey(self.0) };
        }
    }

    fn open(path: &str) -> Option<Key> {
        let mut hkey = HKEY::default();
        let subkey = HSTRING::from(path);
        // SAFETY: `subkey` is NUL-terminated and outlives the call, `hkey` receives the handle.
        let status =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &subkey, None, KEY_READ, &mut hkey) };
        status.is_ok().then_some(Key(hkey))
    }

    impl Registry for WindowsRegistry {
        fn subkeys(&self, path: &str) -> Vec<String> {
            let Some(key) = open(path) else {
                return Vec::new();
            };
            let mut names = Vec::new();
            let mut buffer = vec![0u16; MAX_KEY_NAME];
            for index in 0.. {
                let mut len = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
                // SAFETY: `buffer` holds `len` UTF-16 units and outlives the call.
                let status = unsafe {
                    RegEnumKeyExW(
                        key.0,
                        index,
                        Some(PWSTR(buffer.as_mut_ptr())),
                        &mut len,
                        None,
                        None,
                        None,
                        None,
                    )
                };
                if status == ERROR_NO_MORE_ITEMS || status.is_err() {
                    break;
                }
                let len = usize::try_from(len).unwrap_or(0).min(buffer.len());
                names.push(String::from_utf16_lossy(&buffer[..len]));
            }
            names
        }

        fn values(&self, path: &str) -> Option<Vec<(String, u64)>> {
            let key = open(path)?;
            let mut values = Vec::new();
            let mut name = vec![0u16; MAX_VALUE_NAME];
            for index in 0.. {
                let mut name_len = u32::try_from(name.len()).unwrap_or(u32::MAX);
                let mut kind = 0u32;
                let mut data = [0u8; 8];
                let mut data_len = 8u32;
                // SAFETY: `name` holds `name_len` UTF-16 units and `data` holds `data_len` bytes, both outlive the call.
                let status = unsafe {
                    RegEnumValueW(
                        key.0,
                        index,
                        Some(PWSTR(name.as_mut_ptr())),
                        &mut name_len,
                        None,
                        Some(&mut kind),
                        Some(data.as_mut_ptr()),
                        Some(&mut data_len),
                    )
                };
                if status == ERROR_NO_MORE_ITEMS {
                    break;
                }
                if status == ERROR_MORE_DATA {
                    continue;
                }
                if status.is_err() {
                    break;
                }
                let value = match kind {
                    k if k == REG_DWORD.0 && data_len >= 4 => {
                        u64::from(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
                    }
                    k if k == REG_QWORD.0 && data_len >= 8 => u64::from_le_bytes(data),
                    _ => continue,
                };
                let len = usize::try_from(name_len).unwrap_or(0).min(name.len());
                values.push((String::from_utf16_lossy(&name[..len]), value));
            }
            Some(values)
        }

        fn last_write(&self, path: &str) -> Option<i64> {
            let key = open(path)?;
            let mut written = FILETIME::default();
            // SAFETY: `written` is a valid FILETIME that outlives the call, every other output is skipped.
            let status = unsafe {
                RegQueryInfoKeyW(
                    key.0,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(&mut written),
                )
            };
            if status.is_err() {
                return None;
            }
            let ticks =
                (u64::from(written.dwHighDateTime) << 32) | u64::from(written.dwLowDateTime);
            i64::try_from(ticks / FILETIME_TICKS_PER_SECOND)
                .ok()
                .map(|seconds| seconds - FILETIME_UNIX_OFFSET)
        }
    }
}

#[cfg(not(windows))]
impl Registry for WindowsRegistry {
    fn subkeys(&self, _path: &str) -> Vec<String> {
        Vec::new()
    }

    fn values(&self, _path: &str) -> Option<Vec<(String, u64)>> {
        None
    }

    fn last_write(&self, _path: &str) -> Option<i64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake() -> MemoryRegistry {
        let mut r = MemoryRegistry::default();
        r.keys
            .insert(LUMAPLAY_ROOT.into(), vec!["alice".into(), "bob".into()]);
        r.keys.insert(
            format!(r"{LUMAPLAY_ROOT}\alice"),
            vec!["4".into(), "no achievements".into(), "635".into()],
        );
        r.keys
            .insert(format!(r"{LUMAPLAY_ROOT}\bob"), vec!["54".into()]);
        r.values.insert(
            format!(r"{LUMAPLAY_ROOT}\alice\4\Achievements"),
            vec![("ACH_2".into(), 0), ("ACH_1".into(), 1)],
        );
        r.values.insert(
            format!(r"{LUMAPLAY_ROOT}\bob\54\Achievements"),
            vec![("ACH_9".into(), 1)],
        );
        r
    }

    #[test]
    fn discover_lists_users_and_ids() {
        let found = discover(&fake());
        let got: Vec<(String, PathBuf)> = found
            .iter()
            .map(|f| {
                let e = f.external.as_ref().unwrap();
                assert_eq!(e.platform, Platform::Ubisoft);
                assert_eq!(f.source_id, "lumaplay");
                assert_eq!(f.format, Format::LumaPlay);
                (e.id.clone(), f.path.clone())
            })
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    "4".into(),
                    PathBuf::from(r"HKCU\SOFTWARE\LumaPlay\alice\4\Achievements")
                ),
                (
                    "54".into(),
                    PathBuf::from(r"HKCU\SOFTWARE\LumaPlay\bob\54\Achievements")
                ),
            ]
        );
    }

    #[test]
    fn snapshot_is_sorted_and_stable() {
        let registry = fake();
        let path = PathBuf::from(r"HKCU\SOFTWARE\LumaPlay\alice\4\Achievements");
        let bytes = snapshot(&registry, &path).unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), "ACH_1=1\nACH_2=0\n");
        assert_eq!(
            snapshot(&registry, &path).unwrap(),
            snapshot(&registry, &path).unwrap()
        );
    }

    #[test]
    fn snapshot_of_missing_or_foreign_key_is_an_error() {
        let registry = fake();
        assert!(
            snapshot(
                &registry,
                Path::new(r"HKCU\SOFTWARE\LumaPlay\x\1\Achievements")
            )
            .is_err()
        );
        assert!(snapshot(&registry, Path::new(r"C:\Games\file.ini")).is_err());
    }

    #[test]
    fn is_registry_path_only_for_hkcu_keys() {
        assert!(is_registry_path(Path::new(
            r"HKCU\SOFTWARE\LumaPlay\a\1\Achievements"
        )));
        assert!(!is_registry_path(Path::new(r"C:\HKCU\file.ini")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_registry_reads_real_key() {
        let keys = WindowsRegistry.subkeys(r"SOFTWARE\Microsoft\Windows\CurrentVersion");
        assert!(!keys.is_empty());
        assert!(
            WindowsRegistry
                .values(r"SOFTWARE\AW\does not exist")
                .is_none()
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_registry_reports_key_write_time() {
        let written = WindowsRegistry
            .last_write(r"SOFTWARE\Microsoft\Windows\CurrentVersion")
            .unwrap();
        assert!(written > 946_684_800);
        assert!(
            WindowsRegistry
                .last_write(r"SOFTWARE\AW\does not exist")
                .is_none()
        );
    }

    #[test]
    fn memory_registry_reports_written_time() {
        let mut r = MemoryRegistry::default();
        r.written.insert("k".into(), 5);
        assert_eq!(r.last_write("k"), Some(5));
        assert_eq!(r.last_write("other"), None);
    }
}
