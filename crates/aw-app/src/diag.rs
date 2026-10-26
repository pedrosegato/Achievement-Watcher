use std::io::Write;
use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;

const KEEP_LOG_FILES: usize = 7;

pub fn init_logging(logs_dir: &Path) -> Option<WorkerGuard> {
    std::fs::create_dir_all(logs_dir).ok()?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("aw")
        .filename_suffix("log")
        .max_log_files(KEEP_LOG_FILES)
        .build(logs_dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tao=warn,wry=warn,notify=warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init()
        .ok()?;
    Some(guard)
}

pub fn log_files(logs_dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(logs_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("aw"))
        })
        .collect();
    files.sort();
    files
}

pub fn log_tail(logs_dir: &Path, lines: usize) -> Vec<String> {
    let Some(newest) = log_files(logs_dir).pop() else {
        return Vec::new();
    };
    let text = std::fs::read(&newest)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

pub fn write_zip(target: &Path, entries: &[(String, Vec<u8>)]) -> Result<(), String> {
    let file = std::fs::File::create(target).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in entries {
        zip.start_file(name.as_str(), options)
            .map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_tail_reads_newest_file_end() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("aw.2026-10-23.log"), "old\n").unwrap();
        std::fs::write(tmp.path().join("aw.2026-10-24.log"), "a\nb\nc\n").unwrap();
        std::fs::write(tmp.path().join("other.txt"), "x").unwrap();
        assert_eq!(log_tail(tmp.path(), 2), vec!["b", "c"]);
        assert_eq!(log_files(tmp.path()).len(), 2);
    }

    #[test]
    fn zip_contains_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("d.zip");
        write_zip(&target, &[("a.txt".into(), b"hello".to_vec())]).unwrap();
        let bytes = std::fs::read(&target).unwrap();
        assert_eq!(&bytes[..2], b"PK");
    }
}
