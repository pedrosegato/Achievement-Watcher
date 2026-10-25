use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_PRESET: &str = "aw-minimal";
const VIDEO_BEFORE: (u32, u32) = (5, 60);
const VIDEO_AFTER_MAX: u32 = 15;
const VIDEO_FPS: [u32; 2] = [30, 60];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoQuality {
    Light,
    #[default]
    Balanced,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Position {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Position {
    pub fn as_str(self) -> &'static str {
        match self {
            Position::TopLeft => "top-left",
            Position::TopCenter => "top-center",
            Position::TopRight => "top-right",
            Position::MiddleLeft => "middle-left",
            Position::MiddleRight => "middle-right",
            Position::BottomLeft => "bottom-left",
            Position::BottomCenter => "bottom-center",
            Position::BottomRight => "bottom-right",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub preset: String,
    pub position: Position,
    pub scale: f64,
    pub sound: bool,
    pub sound_volume: f32,
    pub sound_id: String,
    pub rare_threshold: f64,
    pub screenshot: bool,
    pub screenshot_dir: Option<PathBuf>,
    pub language: Option<String>,
    pub autostart: bool,
    pub silent_after_minutes: i64,
    pub extra_game_roots: Vec<PathBuf>,
    pub extra_game_dirs: Vec<PathBuf>,
    pub read_steam_libraries: bool,
    pub steam_achievements: bool,
    pub steam_popups: bool,
    pub progress_popups: bool,
    pub overlay_hotkey: String,
    pub video: bool,
    pub video_before: u32,
    pub video_after: u32,
    pub video_fps: u32,
    pub video_quality: VideoQuality,
    pub auto_update: bool,
    pub ffmpeg_path: Option<PathBuf>,
    pub video_dir: Option<PathBuf>,
    pub hide_hidden_achievements: bool,
    pub ui_language: String,
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            preset: DEFAULT_PRESET.into(),
            position: Position::BottomRight,
            scale: 1.0,
            sound: true,
            sound_volume: 0.8,
            sound_id: crate::sounds::DEFAULT_PACK.into(),
            rare_threshold: 10.0,
            screenshot: true,
            screenshot_dir: None,
            language: None,
            autostart: true,
            silent_after_minutes: 5,
            extra_game_roots: Vec::new(),
            extra_game_dirs: Vec::new(),
            read_steam_libraries: true,
            steam_achievements: true,
            steam_popups: false,
            progress_popups: true,
            overlay_hotkey: crate::overlay::DEFAULT_HOTKEY.into(),
            video: false,
            video_before: 10,
            video_after: 5,
            video_fps: 30,
            video_quality: VideoQuality::Balanced,
            auto_update: true,
            ffmpeg_path: None,
            video_dir: None,
            hide_hidden_achievements: true,
            ui_language: crate::i18n::AUTO.into(),
            theme: Theme::System,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> (Settings, bool) {
        let Ok(bytes) = std::fs::read(path) else {
            return (Settings::default(), true);
        };
        match serde_json::from_slice::<Settings>(&bytes) {
            Ok(settings) => (settings.sanitized(), false),
            Err(error) => {
                tracing::warn!("settings file is invalid, using defaults: {error}");
                let _ = std::fs::rename(path, path.with_extension("json.bad"));
                (Settings::default(), false)
            }
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&temp, path)
    }

    pub fn sanitized(mut self) -> Settings {
        self.scale = if self.scale.is_finite() {
            self.scale.clamp(0.5, 3.0)
        } else {
            1.0
        };
        self.sound_volume = if self.sound_volume.is_finite() {
            self.sound_volume.clamp(0.0, 1.0)
        } else {
            0.8
        };
        self.silent_after_minutes = self.silent_after_minutes.clamp(0, 24 * 60);
        self.video_before = self.video_before.clamp(VIDEO_BEFORE.0, VIDEO_BEFORE.1);
        self.video_after = self.video_after.min(VIDEO_AFTER_MAX);
        if !VIDEO_FPS.contains(&self.video_fps) {
            self.video_fps = VIDEO_FPS[0];
        }
        self.rare_threshold = if self.rare_threshold.is_finite() {
            self.rare_threshold.clamp(0.0, 100.0)
        } else {
            10.0
        };
        if self.preset.trim().is_empty() {
            self.preset = DEFAULT_PRESET.into();
        }
        if !crate::i18n::is_preference(&self.ui_language) {
            self.ui_language = crate::i18n::AUTO.into();
        }
        if self.overlay_hotkey.trim().is_empty() {
            self.overlay_hotkey = Settings::default().overlay_hotkey;
        }
        if self.sound_id.trim().is_empty() {
            self.sound_id = Settings::default().sound_id;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_clip_settings_stay_in_range() {
        let defaults = Settings::default();
        assert_eq!(
            (
                defaults.video_before,
                defaults.video_after,
                defaults.video_fps
            ),
            (10, 5, 30)
        );
        assert_eq!(defaults.video_quality, VideoQuality::Balanced);
        let low = Settings {
            video_before: 2,
            video_after: 40,
            video_fps: 45,
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(
            (low.video_before, low.video_after, low.video_fps),
            (5, 15, 30)
        );
        let high = Settings {
            video_before: 99,
            video_fps: 60,
            ..Settings::default()
        }
        .sanitized();
        assert_eq!((high.video_before, high.video_fps), (60, 60));
        let parsed: Settings = serde_json::from_str(r#"{"videoQuality":"high"}"#).unwrap();
        assert_eq!(parsed.video_quality, VideoQuality::High);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let (settings, first_run) = Settings::load(&tmp.path().join("settings.json"));
        assert_eq!(settings, Settings::default());
        assert!(first_run);
    }

    #[test]
    fn partial_file_fills_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"preset":"mine","position":"top-left","sound":false}"#,
        )
        .unwrap();
        let (settings, first_run) = Settings::load(&path);
        assert!(!first_run);
        assert_eq!(settings.preset, "mine");
        assert_eq!(settings.position, Position::TopLeft);
        assert!(!settings.sound);
        assert!(settings.screenshot);
        assert_eq!(settings.silent_after_minutes, 5);
    }

    #[test]
    fn invalid_file_is_backed_up() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{nope").unwrap();
        let (settings, _) = Settings::load(&path);
        assert_eq!(settings, Settings::default());
        assert!(tmp.path().join("settings.json.bad").is_file());
        assert!(!path.exists());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("settings.json");
        let settings = Settings {
            preset: "x".into(),
            scale: 1.5,
            extra_game_roots: vec![PathBuf::from(r"C:\Jogos")],
            ..Settings::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).0, settings);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, r#"{"soundFile":"C:\\x.wav","soundId":"Xbox"}"#).unwrap();
        assert_eq!(Settings::load(&path).0.sound_id, "Xbox");
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let s = Settings {
            scale: 99.0,
            sound_volume: -1.0,
            silent_after_minutes: -5,
            preset: " ".into(),
            sound_id: String::new(),
            ui_language: "klingon".into(),
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(s.scale, 3.0);
        assert_eq!(s.sound_volume, 0.0);
        assert_eq!(s.silent_after_minutes, 0);
        assert_eq!(s.preset, "aw-minimal");
        assert_eq!(s.sound_id, "achievement-watcher");
        assert_eq!(s.ui_language, "auto");
    }

    #[test]
    fn theme_defaults_to_system_and_round_trips() {
        assert_eq!(Settings::default().theme, Theme::System);
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, r#"{"theme":"light","position":"middle-right"}"#).unwrap();
        let loaded = Settings::load(&path).0;
        assert_eq!(loaded.theme, Theme::Light);
        assert_eq!(loaded.position, Position::MiddleRight);
        assert_eq!(Position::MiddleLeft.as_str(), "middle-left");
    }
}
