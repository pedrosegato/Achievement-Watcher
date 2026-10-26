use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

const BACKOFF_MS: [u64; 5] = [25, 50, 100, 200, 400];
const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;

pub fn read_with_retry(path: &Path, budget: Duration) -> io::Result<Vec<u8>> {
    let start = Instant::now();
    let mut attempt = 0;
    loop {
        match std::fs::read(path) {
            Ok(bytes) => return Ok(bytes),
            Err(error) if is_transient(&error) => {
                let wait = Duration::from_millis(BACKOFF_MS[attempt.min(BACKOFF_MS.len() - 1)]);
                if start.elapsed() + wait > budget {
                    return Err(error);
                }
                std::thread::sleep(wait);
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn is_transient(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
    ) || error.kind() == io::ErrorKind::PermissionDenied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.ini");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(read_with_retry(&p, Duration::from_secs(1)).unwrap(), b"abc");
    }

    #[test]
    fn missing_file_fails_fast() {
        let tmp = tempfile::tempdir().unwrap();
        let start = Instant::now();
        assert!(read_with_retry(&tmp.path().join("nope"), Duration::from_secs(1)).is_err());
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[cfg(windows)]
    #[test]
    fn locked_file_is_retried_then_reported() {
        use std::os::windows::fs::OpenOptionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("locked.ini");
        std::fs::write(&p, b"data").unwrap();

        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&p)
            .unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(120));
            drop(lock);
        });
        assert_eq!(
            read_with_retry(&p, Duration::from_secs(1)).unwrap(),
            b"data"
        );
        releaser.join().unwrap();

        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&p)
            .unwrap();
        let start = Instant::now();
        assert!(read_with_retry(&p, Duration::from_millis(300)).is_err());
        assert!(start.elapsed() <= Duration::from_millis(600));
        drop(lock);
    }

    #[test]
    fn fnv_matches_reference() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
