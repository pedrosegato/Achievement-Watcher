use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use xcap::image::RgbaImage;

const CAPTURE_FOLDER: &str = "AchievementWatcher";

const MAX_NAME: usize = 120;

pub const CAPTURE_DELAY: Duration = Duration::from_secs(2);

pub fn later(delay: Duration, job: impl FnOnce() + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("aw-screenshot".into())
        .spawn(move || {
            std::thread::sleep(delay);
            job();
        });
    if let Err(error) = spawned {
        tracing::warn!("screenshot thread failed: {error}");
    }
}

pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    let mut out: String = trimmed.chars().take(MAX_NAME).collect();
    out = out.trim_end_matches(['.', ' ']).to_owned();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3",
    ];
    if out.is_empty() {
        "Unknown".into()
    } else if reserved.iter().any(|r| r.eq_ignore_ascii_case(&out)) {
        format!("_{out}")
    } else {
        out
    }
}

pub fn unique_path(dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{extension}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).{extension}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

pub fn default_capture_root(base: &Path) -> PathBuf {
    base.join(CAPTURE_FOLDER)
}

pub struct WindowCandidate {
    pub pid: u32,
    pub width: u32,
    pub height: u32,
    pub minimized: bool,
}

pub fn game_window(candidates: &[WindowCandidate], pids: &HashSet<u32>) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| pids.contains(&c.pid) && !c.minimized && c.width > 0 && c.height > 0)
        .max_by_key(|(_, c)| u64::from(c.width) * u64::from(c.height))
        .map(|(index, _)| index)
}

fn game_image(pids: &HashSet<u32>) -> Option<RgbaImage> {
    let windows = xcap::Window::all().ok()?;
    let candidates: Vec<WindowCandidate> = windows
        .iter()
        .map(|w| WindowCandidate {
            pid: w.pid().unwrap_or(0),
            width: w.width().unwrap_or(0),
            height: w.height().unwrap_or(0),
            minimized: w.is_minimized().unwrap_or(true),
        })
        .collect();
    let window = &windows[game_window(&candidates, pids)?];
    window
        .capture_image()
        .inspect_err(|error| tracing::warn!("game window capture failed: {error}"))
        .ok()
}

fn monitor_image() -> Result<RgbaImage, String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let monitor = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or("no monitor")?;
    monitor.capture_image().map_err(|e| e.to_string())
}

pub fn capture(
    root: &Path,
    game: &str,
    achievement: &str,
    pids: &HashSet<u32>,
) -> Result<PathBuf, String> {
    let image = match game_image(pids) {
        Some(image) => image,
        None => monitor_image()?,
    };
    let dir = root.join(sanitize(game));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = unique_path(&dir, &sanitize(achievement), "png");
    image.save(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_runs_off_the_caller_after_the_delay() {
        let (tx, rx) = std::sync::mpsc::channel();
        let start = std::time::Instant::now();
        later(std::time::Duration::from_millis(80), move || {
            let _ = tx.send(start.elapsed());
        });
        assert!(start.elapsed() < std::time::Duration::from_millis(40));
        let ran_after = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert!(ran_after >= std::time::Duration::from_millis(80));
    }

    #[test]
    fn game_window_is_the_largest_visible_one_the_game_owns() {
        let window = |pid, width, height, minimized| WindowCandidate {
            pid,
            width,
            height,
            minimized,
        };
        let candidates = [
            window(10, 300, 200, false),
            window(99, 2560, 1440, false),
            window(11, 2560, 1440, true),
            window(11, 1920, 1080, false),
            window(10, 0, 0, false),
        ];
        let game = std::collections::HashSet::from([10, 11]);
        assert_eq!(game_window(&candidates, &game), Some(3));
        assert_eq!(
            game_window(&candidates, &std::collections::HashSet::new()),
            None
        );
    }

    #[test]
    fn default_capture_root_adds_app_folder() {
        assert_eq!(
            default_capture_root(Path::new(r"C:\Pics")),
            PathBuf::from(r"C:\Pics\AchievementWatcher")
        );
    }

    #[test]
    fn sanitize_file_names() {
        assert_eq!(sanitize("ELDEN RING"), "ELDEN RING");
        assert_eq!(sanitize("A: B/C\\D?*"), "A_ B_C_D__");
        assert_eq!(sanitize("  name. . "), "name");
        assert_eq!(sanitize(""), "Unknown");
        assert_eq!(sanitize("con"), "_con");
        assert_eq!(sanitize(&"x".repeat(300)).len(), MAX_NAME);
    }

    #[test]
    fn unique_path_adds_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        let first = unique_path(tmp.path(), "A", "png");
        assert_eq!(first, tmp.path().join("A.png"));
        std::fs::write(&first, "x").unwrap();
        assert_eq!(
            unique_path(tmp.path(), "A", "png"),
            tmp.path().join("A (2).png")
        );
        std::fs::write(tmp.path().join("A (2).png"), "x").unwrap();
        assert_eq!(
            unique_path(tmp.path(), "A", "png"),
            tmp.path().join("A (3).png")
        );
    }
}
