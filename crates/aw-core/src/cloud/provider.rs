use std::path::{Path, PathBuf};
use std::time::Duration;

const FRESH_CLIENT: Duration = Duration::from_secs(180);
const QUIET: Duration = Duration::from_secs(5);
const WAIT_CAP: Duration = Duration::from_secs(30);
const GOOGLE_DRIVE_FOLDERS: &[&str] = &["my drive", "meu drive", "shared drives"];
const ONEDRIVE_VARS: &[&str] = &["OneDrive", "OneDriveConsumer", "OneDriveCommercial"];
const DROPBOX_INFO: &[&str] = &["APPDATA", "LOCALAPPDATA"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    GoogleDrive,
    OneDrive,
    Dropbox,
}

impl Provider {
    pub fn client_exe(self) -> &'static str {
        match self {
            Provider::GoogleDrive => "GoogleDriveFS.exe",
            Provider::OneDrive => "OneDrive.exe",
            Provider::Dropbox => "Dropbox.exe",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderHints {
    pub onedrive_roots: Vec<PathBuf>,
    pub dropbox_roots: Vec<PathBuf>,
}

impl ProviderHints {
    pub fn from_env() -> ProviderHints {
        let onedrive_roots = ONEDRIVE_VARS
            .iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .collect();
        let dropbox_roots = DROPBOX_INFO
            .iter()
            .filter_map(std::env::var_os)
            .filter_map(|base| {
                std::fs::read(PathBuf::from(base).join("Dropbox").join("info.json")).ok()
            })
            .filter_map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .flat_map(|info| {
                info.as_object()
                    .into_iter()
                    .flat_map(|accounts| accounts.values())
                    .filter_map(|account| account.get("path")?.as_str().map(PathBuf::from))
                    .collect::<Vec<_>>()
            })
            .collect();
        ProviderHints {
            onedrive_roots,
            dropbox_roots,
        }
    }
}

fn lower_parts(path: &Path) -> Vec<String> {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}

fn under(folder: &[String], root: &Path) -> bool {
    let root = lower_parts(root);
    !root.is_empty() && folder.starts_with(&root)
}

pub fn provider_for(folder: &Path, hints: &ProviderHints) -> Option<Provider> {
    let parts = lower_parts(folder);
    if hints.onedrive_roots.iter().any(|root| under(&parts, root)) {
        return Some(Provider::OneDrive);
    }
    if hints.dropbox_roots.iter().any(|root| under(&parts, root))
        || parts.iter().any(|part| part == "dropbox")
    {
        return Some(Provider::Dropbox);
    }
    parts
        .iter()
        .any(|part| GOOGLE_DRIVE_FOLDERS.contains(&part.as_str()))
        .then_some(Provider::GoogleDrive)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveWait {
    Ready,
    Wait,
    ClientClosed,
}

pub fn drive_wait(
    client_started_ago: Option<Duration>,
    quiet_for: Duration,
    waited: Duration,
) -> DriveWait {
    match client_started_ago {
        None => DriveWait::ClientClosed,
        Some(age) if age > FRESH_CLIENT || quiet_for >= QUIET || waited >= WAIT_CAP => {
            DriveWait::Ready
        }
        Some(_) => DriveWait::Wait,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn google_drive_folder_is_detected() {
        let hints = ProviderHints::default();
        for folder in [r"G:\My Drive\AchievementWatcher", r"G:\Meu Drive\AW"] {
            assert_eq!(
                provider_for(Path::new(folder), &hints),
                Some(Provider::GoogleDrive),
                "{folder}"
            );
        }
    }

    #[test]
    fn onedrive_folder_is_detected_by_env_root() {
        let hints = ProviderHints {
            onedrive_roots: vec![PathBuf::from(r"C:\Users\p\OneDrive")],
            ..Default::default()
        };
        assert_eq!(
            provider_for(Path::new(r"C:\Users\p\OneDrive\Games\AW"), &hints),
            Some(Provider::OneDrive)
        );
        assert_eq!(
            provider_for(Path::new(r"C:\Users\p\OneDriveX\AW"), &hints),
            None
        );
    }

    #[test]
    fn dropbox_folder_is_detected() {
        let hints = ProviderHints {
            dropbox_roots: vec![PathBuf::from(r"D:\Dropbox")],
            ..Default::default()
        };
        assert_eq!(
            provider_for(Path::new(r"D:\Dropbox\AW"), &hints),
            Some(Provider::Dropbox)
        );
    }

    #[test]
    fn plain_folder_has_no_provider() {
        assert_eq!(
            provider_for(Path::new(r"D:\Saves\AW"), &ProviderHints::default()),
            None
        );
    }

    #[test]
    fn closed_client_is_reported() {
        assert_eq!(drive_wait(None, secs(0), secs(0)), DriveWait::ClientClosed);
    }

    #[test]
    fn old_client_is_ready_at_once() {
        assert_eq!(
            drive_wait(Some(secs(181)), secs(0), secs(0)),
            DriveWait::Ready
        );
    }

    #[test]
    fn fresh_client_waits_for_quiet_folder() {
        assert_eq!(
            drive_wait(Some(secs(10)), secs(2), secs(3)),
            DriveWait::Wait
        );
        assert_eq!(
            drive_wait(Some(secs(10)), secs(5), secs(6)),
            DriveWait::Ready
        );
    }

    #[test]
    fn fresh_client_gives_up_waiting_after_the_cap() {
        assert_eq!(
            drive_wait(Some(secs(10)), secs(1), secs(30)),
            DriveWait::Ready
        );
    }

    #[test]
    fn client_names() {
        assert_eq!(Provider::GoogleDrive.client_exe(), "GoogleDriveFS.exe");
        assert_eq!(Provider::OneDrive.client_exe(), "OneDrive.exe");
        assert_eq!(Provider::Dropbox.client_exe(), "Dropbox.exe");
    }
}
