use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudFileState {
    InCloud,
    InFolder,
}

pub fn folder_state(dir: &Path) -> CloudFileState {
    let files = files_under(dir);
    if !files.is_empty() && files.iter().all(|file| is_synced_placeholder(file)) {
        CloudFileState::InCloud
    } else {
        CloudFileState::InFolder
    }
}

pub fn folder_size(dir: &Path) -> u64 {
    files_under(dir)
        .iter()
        .filter_map(|file| std::fs::symlink_metadata(file).ok())
        .map(|meta| meta.len())
        .sum()
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current).into_iter().flatten().flatten() {
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push(entry.path()),
                Ok(kind) if kind.is_file() => out.push(entry.path()),
                _ => {}
            }
        }
    }
    out
}

#[cfg(windows)]
fn is_synced_placeholder(file: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::CloudFilters::{
        CF_PLACEHOLDER_STATE_IN_SYNC, CF_PLACEHOLDER_STATE_PLACEHOLDER,
        CfGetPlaceholderStateFromAttributeTag,
    };
    use windows::Win32::Storage::FileSystem::{FindClose, FindFirstFileW, WIN32_FIND_DATAW};
    use windows::core::PCWSTR;

    let wide: Vec<u16> = file.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut data = WIN32_FIND_DATAW::default();
    // SAFETY: `wide` is NUL-terminated and outlives the call; `data` is a valid out buffer.
    let Ok(handle) = (unsafe { FindFirstFileW(PCWSTR(wide.as_ptr()), &mut data) }) else {
        return false;
    };
    // SAFETY: `handle` came from a successful FindFirstFileW and is closed exactly once.
    let _ = unsafe { FindClose(handle) };
    // SAFETY: plain value-in, value-out query with no pointers.
    let state =
        unsafe { CfGetPlaceholderStateFromAttributeTag(data.dwFileAttributes, data.dwReserved0) };
    state.0 & CF_PLACEHOLDER_STATE_PLACEHOLDER.0 != 0
        && state.0 & CF_PLACEHOLDER_STATE_IN_SYNC.0 != 0
}

#[cfg(not(windows))]
fn is_synced_placeholder(_file: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_folder_is_in_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), "abc").unwrap();
        assert_eq!(folder_state(dir.path()), CloudFileState::InFolder);
    }

    #[test]
    fn empty_folder_is_in_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(folder_state(dir.path()), CloudFileState::InFolder);
        assert_eq!(
            folder_state(&dir.path().join("missing")),
            CloudFileState::InFolder
        );
    }

    #[test]
    fn folder_size_sums_recursively() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), "abc").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("b.bin"), "de").unwrap();
        assert_eq!(folder_size(dir.path()), 5);
        assert_eq!(folder_size(&dir.path().join("missing")), 0);
    }

    #[test]
    #[ignore = "needs a OneDrive folder in AW_ONEDRIVE_TEST_DIR"]
    fn onedrive_folder_reports_in_cloud() {
        let dir = std::env::var("AW_ONEDRIVE_TEST_DIR").unwrap();
        assert_eq!(
            folder_state(std::path::Path::new(&dir)),
            CloudFileState::InCloud
        );
    }
}
