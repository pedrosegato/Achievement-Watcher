use std::path::{Path, PathBuf};

const CAPTURE_FOLDER: &str = "AchievementWatcher";

const MAX_NAME: usize = 120;

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

pub fn capture(root: &Path, game: &str, achievement: &str) -> Result<PathBuf, String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let monitor = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or("no monitor")?;
    let image = monitor.capture_image().map_err(|e| e.to_string())?;
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
