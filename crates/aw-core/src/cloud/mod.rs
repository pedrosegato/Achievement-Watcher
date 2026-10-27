pub mod capture;
pub mod decide;
pub mod device;
pub mod heads;
pub mod identity;
pub mod key;
pub mod keyed;
pub mod layout;
pub mod lineage;
pub mod locate;
pub mod lock;
pub mod manifest;
pub mod migrate;
pub mod portable;
pub mod provider;
pub mod restore;
pub mod snapshot;
pub mod status;
pub mod steamcloud;
pub mod sync;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("cloud folder is missing")]
    MissingRoot,
    #[error("snapshot {0} is incomplete")]
    Incomplete(String),
    #[error("more than one account folder for {0}")]
    MultipleAccounts(String),
    #[error("cancelled")]
    Cancelled,
    #[error("the save changed while it was being copied")]
    SaveChanged,
}

pub type CloudResult<T> = Result<T, CloudError>;

#[derive(Default)]
pub struct CancelGate {
    cancelled: std::sync::atomic::AtomicBool,
    swap: std::sync::Mutex<()>,
}

impl CancelGate {
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
        drop(
            self.swap
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn begin_swap(&self) -> Option<std::sync::MutexGuard<'_, ()>> {
        let guard = self
            .swap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (!self.is_cancelled()).then_some(guard)
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn random_id(len: usize) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut out = String::new();
    let mut round = 0u32;
    while out.len() < len {
        let mut hasher = Sha256::new();
        hasher.update(nanos.to_le_bytes());
        hasher.update(std::process::id().to_le_bytes());
        hasher.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
        hasher.update(round.to_le_bytes());
        out.push_str(&hex(&hasher.finalize()));
        round += 1;
    }
    out.truncate(len);
    out
}

pub(crate) fn at(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> std::io::Error + '_ {
    move |error| std::io::Error::new(error.kind(), format!("{}: {error}", path.display()))
}

pub(crate) fn physical(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(real) = std::fs::canonicalize(path) {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => std::fs::canonicalize(parent)
            .map(|real| real.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

pub(crate) fn copy_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<u64> {
    std::fs::copy(from, to).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!("copy {} to {}: {error}", from.display(), to.display()),
        )
    })
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn utc_stamp(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}-{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_stamp_formats_minutes() {
        assert_eq!(utc_stamp(1_793_050_200), "2026-10-26_21-30");
        assert_eq!(utc_stamp(0), "1970-01-01_00-00");
        assert_eq!(utc_stamp(951_782_400), "2000-02-29_00-00");
    }

    #[test]
    fn random_ids_are_hex_and_differ() {
        let a = random_id(16);
        let b = random_id(16);
        assert_eq!(a.len(), 16);
        assert!(
            a.bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_ne!(a, b);
    }

    #[test]
    fn cancel_waits_for_a_swap_in_progress() {
        let gate = std::sync::Arc::new(CancelGate::default());
        let swap = gate.begin_swap().expect("not cancelled yet");
        let (tx, rx) = std::sync::mpsc::channel();
        let other = std::sync::Arc::clone(&gate);
        std::thread::spawn(move || {
            other.cancel();
            let _ = tx.send(());
        });
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(200))
                .is_err()
        );
        drop(swap);
        rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert!(gate.is_cancelled());
        assert!(gate.begin_swap().is_none());
    }
}
