use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Url, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::app::AppState;
use crate::i18n;
use crate::popup_queue::PopupQueue;
use crate::presets::{self, DEFAULT_DURATION_MS, PresetInfo};
use crate::settings::{Position, Settings};
use crate::sounds::SoundEvent;
use crate::{shot, sound};

pub const POPUP_LABEL: &str = "popup";
pub const PROTOCOL_ORIGIN: &str = "http://aw.localhost";
pub const DEFAULT_ICON: &str = "aw-icon/_default.svg";
const MARGIN: f64 = 48.0;
const LOAD_GRACE: Duration = Duration::from_millis(900);
const HIDE_SLACK: Duration = Duration::from_millis(900);

pub const SHIM: &str = r#"(() => {
  if (window.__awShim) return;
  window.__awShim = true;
  const notify = [];
  const scale = [];
  const origin = location.origin;
  const mapIcon = (value) => {
    const text = String(value ?? "");
    const at = text.indexOf("aw-icon/");
    return at >= 0 ? `${origin}/icon/${text.slice(at + 8)}` : text;
  };
  const src = Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, "src");
  Object.defineProperty(HTMLImageElement.prototype, "src", {
    configurable: true,
    enumerable: src.enumerable,
    get() { return src.get.call(this); },
    set(value) { src.set.call(this, mapIcon(value)); },
  });
  const setAttribute = Element.prototype.setAttribute;
  Element.prototype.setAttribute = function (name, value) {
    const isSrc = this instanceof HTMLImageElement && String(name).toLowerCase() === "src";
    return setAttribute.call(this, name, isSrc ? mapIcon(value) : value);
  };
  const api = {
    onNotification(cb) { if (typeof cb === "function") notify.push(cb); },
    onAnimationScale(cb) { if (typeof cb === "function") scale.push(cb); },
    closeNotificationWindow() {},
  };
  window.api = api;
  window.electronAPI = api;
  window.__awShow = (payload) => {
    for (const cb of scale) { try { cb(null, payload.animationScale ?? 1); } catch (e) { console.error(e); } }
    for (const cb of notify) { try { cb(payload); } catch (e) { console.error(e); } }
  };
})();"#;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub game: String,
    pub display_name: String,
    pub description: String,
    pub icon_path: String,
    pub icon: String,
    pub scale: f64,
    pub duration: f64,
    pub animation_scale: f64,
    pub position: String,
    pub unlocked_label: String,
    pub percent: Option<f64>,
    pub rare: bool,
    pub platinum: bool,
    pub kind: &'static str,
    pub theme: &'static str,
    pub accent_color: String,
    pub sound_enabled: bool,
    pub sound_volume: f32,
    pub progress: Option<ProgressPayload>,
    pub total_achievements: u32,
    pub game_art: Option<String>,
    pub achievement_icons: Vec<String>,
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProgressPayload {
    pub current: u32,
    pub max: u32,
}

const LABELS: [&str; 11] = [
    "unlocked",
    "rare",
    "platinum",
    "progress",
    "platinumDone",
    "allUnlocked",
    "players",
    "mythic",
    "unlockedAll",
    "progressOf",
    "trophyEarned",
];

fn kind_name(kind: PopupKind) -> &'static str {
    match kind {
        PopupKind::Unlock => "unlock",
        PopupKind::Rare => "rare",
        PopupKind::Platinum => "platinum",
        PopupKind::Progress => "progress",
    }
}

pub fn build_payload(
    request: &PopupRequest,
    settings: &Settings,
    locale: &str,
    theme: &'static str,
    accent: &str,
) -> Payload {
    let (current, max) = request.progress.unwrap_or((0, 0));
    let vars = [
        ("game", request.game.clone()),
        ("n", request.total_achievements.to_string()),
        (
            "pct",
            request
                .percent
                .map(|p| i18n::percent(locale, p))
                .unwrap_or_default(),
        ),
        ("current", current.to_string()),
        ("max", max.to_string()),
    ];
    let labels = LABELS
        .iter()
        .map(|key| {
            (
                (*key).to_owned(),
                i18n::text_with(locale, &format!("popup.{key}"), &vars),
            )
        })
        .collect();
    Payload {
        game: request.game.clone(),
        display_name: request.display_name.clone(),
        description: request.description.clone(),
        icon_path: request.icon_path.clone(),
        icon: request.icon_path.clone(),
        scale: settings.scale,
        duration: 1.0,
        animation_scale: 1.0,
        position: settings.position.as_str().into(),
        unlocked_label: i18n::text(locale, label(request.kind)),
        percent: request.percent,
        rare: request.kind == PopupKind::Rare
            || request.percent.is_some_and(|p| p < settings.rare_threshold),
        platinum: request.kind == PopupKind::Platinum,
        kind: kind_name(request.kind),
        theme,
        accent_color: accent.to_owned(),
        sound_enabled: settings.sound,
        sound_volume: settings.sound_volume,
        progress: request
            .progress
            .map(|(current, max)| ProgressPayload { current, max }),
        total_achievements: request.total_achievements,
        game_art: (request.app_id != 0)
            .then(|| crate::art::art_url(request.app_id, crate::art::ArtKind::Header)),
        achievement_icons: request.achievement_icons.clone(),
        labels,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PopupKind {
    Unlock,
    Rare,
    Platinum,
    Progress,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PopupRequest {
    pub game: String,
    pub display_name: String,
    pub description: String,
    pub icon_path: String,
    pub screenshot: bool,
    pub preset_override: Option<String>,
    pub kind: PopupKind,
    pub percent: Option<f64>,
    pub preview: bool,
    pub app_id: u32,
    pub api_name: String,
    pub progress: Option<(u32, u32)>,
    pub total_achievements: u32,
    pub achievement_icons: Vec<String>,
}

pub fn classify(percent: Option<f64>, threshold: f64, completes_game: bool) -> PopupKind {
    if completes_game {
        PopupKind::Platinum
    } else if percent.is_some_and(|p| p < threshold) {
        PopupKind::Rare
    } else {
        PopupKind::Unlock
    }
}

fn sound_event(kind: PopupKind) -> Option<SoundEvent> {
    match kind {
        PopupKind::Unlock => Some(SoundEvent::Unlock),
        PopupKind::Rare => Some(SoundEvent::Rare),
        PopupKind::Platinum => Some(SoundEvent::Platinum),
        PopupKind::Progress => None,
    }
}

fn label(kind: PopupKind) -> &'static str {
    match kind {
        PopupKind::Unlock => i18n::keys::UNLOCKED,
        PopupKind::Rare => i18n::keys::RARE,
        PopupKind::Platinum => i18n::keys::PLATINUM,
        PopupKind::Progress => i18n::keys::PROGRESS,
    }
}

pub fn show_script(payload: &Payload) -> String {
    let json = serde_json::to_string(payload).unwrap_or_else(|_| "{}".into());
    format!("window.__awShow && window.__awShow({json});")
}

pub fn encode_segment(segment: &str) -> String {
    segment
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

pub fn preset_url(id: &str) -> String {
    format!("{PROTOCOL_ORIGIN}/preset/{}/index.html", encode_segment(id))
}

pub fn edge_inset(bleed: f64, scale: f64, factor: f64) -> i64 {
    ((MARGIN - bleed * scale) * factor).round() as i64
}

pub fn position_for(
    position: Position,
    monitor_origin: (i32, i32),
    monitor_size: (u32, u32),
    window: (u32, u32),
    inset: (i64, i64),
) -> (i32, i32) {
    let (mx, my) = monitor_origin;
    let free_w = i64::from(monitor_size.0) - i64::from(window.0);
    let free_h = i64::from(monitor_size.1) - i64::from(window.1);
    let left = inset.0;
    let center = free_w / 2;
    let right = free_w - inset.0;
    let top = inset.1;
    let middle = free_h / 2;
    let bottom = free_h - inset.1;
    let (x, y) = match position {
        Position::TopLeft => (left, top),
        Position::TopCenter => (center, top),
        Position::TopRight => (right, top),
        Position::MiddleLeft => (left, middle),
        Position::MiddleRight => (right, middle),
        Position::BottomLeft => (left, bottom),
        Position::BottomCenter => (center, bottom),
        Position::BottomRight => (right, bottom),
    };
    let clamp = |v: i64| i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX });
    (mx.saturating_add(clamp(x)), my.saturating_add(clamp(y)))
}

pub fn create_window(app: &AppHandle, preset_id: &str) -> tauri::Result<WebviewWindow> {
    let url = Url::parse(&preset_url(preset_id)).map_err(|e| tauri::Error::Anyhow(e.into()))?;
    let window = WebviewWindowBuilder::new(app, POPUP_LABEL, WebviewUrl::External(url))
        .title("Achievement Watcher notification")
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .shadow(false)
        .resizable(false)
        .visible(false)
        .inner_size(450.0, 120.0)
        .initialization_script(SHIM)
        .build()?;
    let _ = window.set_ignore_cursor_events(true);
    Ok(window)
}

pub fn run_worker(app: AppHandle, queue: Arc<PopupQueue>) {
    let mut loaded: Option<(String, Instant)> = None;
    loop {
        let request = queue.next();
        if let Err(error) = show_one(&app, &queue, &request, &mut loaded) {
            tracing::warn!("popup failed: {error}");
        }
    }
}

fn resolve_preset(app: &AppHandle, wanted: &str) -> Option<PresetInfo> {
    let state = app.state::<AppState>();
    let builtin = &state.paths.builtin_presets;
    let user = &state.paths.user_presets;
    presets::find(builtin, user, wanted)
        .or_else(|| presets::find(builtin, user, crate::settings::DEFAULT_PRESET))
}

fn show_one(
    app: &AppHandle,
    queue: &PopupQueue,
    request: &PopupRequest,
    loaded: &mut Option<(String, Instant)>,
) -> Result<(), String> {
    let settings: Settings = app.state::<AppState>().settings_snapshot();
    let (window, preset) = ready_window(app, request, &settings, loaded)?;
    schedule_screenshot(app, request, &settings);
    play_sound(app, request, &settings);
    let (width, height, duration_ms, bleed) =
        preset
            .as_ref()
            .map_or((450, 120, DEFAULT_DURATION_MS, (0.0, 0.0)), |p| {
                (
                    p.width,
                    p.height,
                    p.durations.for_kind(request.kind),
                    p.bleed,
                )
            });
    place(app, &window, (width, height), bleed, &settings)?;
    window.show().map_err(|e| e.to_string())?;
    let _ = window.set_always_on_top(true);
    let _ = window.set_ignore_cursor_events(true);
    let payload = build_payload(
        request,
        &settings,
        app.state::<AppState>().ui_locale(),
        crate::system_theme::effective_theme(settings.theme),
        &crate::system_theme::windows_accent(),
    );
    window
        .eval(show_script(&payload))
        .map_err(|e| e.to_string())?;
    queue.wait_or_interrupt(Duration::from_millis(duration_ms) + HIDE_SLACK);
    window.hide().map_err(|e| e.to_string())?;
    Ok(())
}

fn ready_window(
    app: &AppHandle,
    request: &PopupRequest,
    settings: &Settings,
    loaded: &mut Option<(String, Instant)>,
) -> Result<(WebviewWindow, Option<PresetInfo>), String> {
    let wanted = if request.kind == PopupKind::Progress {
        crate::settings::DEFAULT_PRESET.to_owned()
    } else {
        request
            .preset_override
            .clone()
            .unwrap_or_else(|| settings.preset.clone())
    };
    let preset = resolve_preset(app, &wanted);
    let preset_id = preset
        .as_ref()
        .map_or_else(|| wanted.clone(), |p| p.id.clone());
    let window = match app.get_webview_window(POPUP_LABEL) {
        Some(window) => window,
        None => {
            *loaded = Some((preset_id.clone(), Instant::now()));
            create_window(app, &preset_id).map_err(|e| e.to_string())?
        }
    };
    if loaded.as_ref().is_none_or(|(id, _)| *id != preset_id) {
        let url = Url::parse(&preset_url(&preset_id)).map_err(|e| e.to_string())?;
        window.navigate(url).map_err(|e| e.to_string())?;
        *loaded = Some((preset_id, Instant::now()));
    }
    if let Some((_, since)) = loaded {
        let elapsed = since.elapsed();
        if elapsed < LOAD_GRACE {
            std::thread::sleep(LOAD_GRACE - elapsed);
        }
    }
    Ok((window, preset))
}

fn schedule_screenshot(app: &AppHandle, request: &PopupRequest, settings: &Settings) {
    if !request.screenshot || !settings.screenshot {
        return;
    }
    let root = settings.screenshot_dir.clone().or_else(|| {
        app.path()
            .picture_dir()
            .ok()
            .map(|d| shot::default_capture_root(&d))
    });
    let Some(root) = root else {
        return;
    };
    let game = request.game.clone();
    let achievement = request.display_name.clone();
    let pids = app.state::<AppState>().pids_of(request.app_id);
    shot::later(shot::CAPTURE_DELAY, move || {
        match shot::capture(&root, &game, &achievement, &pids) {
            Ok(path) => tracing::info!("screenshot saved to {}", path.display()),
            Err(error) => tracing::warn!("screenshot failed: {error}"),
        }
    });
}

fn play_sound(app: &AppHandle, request: &PopupRequest, settings: &Settings) {
    if settings.sound
        && let Some(event) = sound_event(request.kind)
    {
        let source = app
            .state::<AppState>()
            .sound_source(&settings.sound_id, event);
        sound::play(settings.sound_volume, source);
    }
}

fn place(
    app: &AppHandle,
    window: &WebviewWindow,
    (width, height): (u32, u32),
    bleed: (f64, f64),
    settings: &Settings,
) -> Result<(), String> {
    let monitor = app
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("no primary monitor")?;
    let factor = monitor.scale_factor();
    let size = (
        (f64::from(width) * settings.scale * factor).round() as u32,
        (f64::from(height) * settings.scale * factor).round() as u32,
    );
    let area = monitor.work_area();
    let origin = (area.position.x, area.position.y);
    let screen = (area.size.width, area.size.height);
    let (x, y) = position_for(
        settings.position,
        origin,
        screen,
        size,
        (
            edge_inset(bleed.0, settings.scale, factor),
            edge_inset(bleed.1, settings.scale, factor),
        ),
    );
    window
        .set_size(PhysicalSize::new(size.0, size.1))
        .map_err(|e| e.to_string())?;
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(kind: PopupKind) -> PopupRequest {
        PopupRequest {
            game: "Stardew Valley".into(),
            display_name: "A Complete Collection".into(),
            description: "Complete the museum collection.".into(),
            icon_path: "aw-icon/413150/a.jpg".into(),
            screenshot: false,
            preset_override: None,
            kind,
            percent: Some(7.2),
            preview: false,
            app_id: 413150,
            api_name: "Museum".into(),
            progress: None,
            total_achievements: 49,
            achievement_icons: vec!["aw-icon/413150/a.jpg".into()],
        }
    }

    #[test]
    fn payload_carries_effective_theme() {
        let settings = Settings::default();
        let payload = build_payload(
            &sample(PopupKind::Rare),
            &settings,
            "pt-BR",
            "dark",
            "#107C10",
        );
        assert_eq!(payload.theme, "dark");
        assert_eq!(payload.kind, "rare");
        assert_eq!(payload.accent_color, "#107C10");
        assert_eq!(
            payload.labels["players"],
            "7,2% dos jogadores têm esta conquista"
        );
        assert_eq!(
            payload.labels["platinumDone"],
            "Você platinou Stardew Valley"
        );
        assert_eq!(
            payload.labels["allUnlocked"],
            "Todas as 49 conquistas desbloqueadas"
        );
        assert_eq!(payload.labels["trophyEarned"], "Troféu conquistado!");
        assert_eq!(
            payload.game_art.as_deref(),
            Some("http://aw.localhost/art/413150/header")
        );
    }

    #[test]
    fn payload_serializes_camel_case_fields() {
        let mut request = sample(PopupKind::Progress);
        request.progress = Some((75, 100));
        let json = show_script(&build_payload(
            &request,
            &Settings::default(),
            "en",
            "light",
            "#0078D4",
        ));
        for field in [
            "\"accentColor\"",
            "\"soundVolume\"",
            "\"soundEnabled\"",
            "\"achievementIcons\"",
            "\"labels\"",
            "\"totalAchievements\"",
            "\"progress\":{\"current\":75,\"max\":100}",
        ] {
            assert!(json.contains(field), "{field} missing in {json}");
        }
    }

    #[test]
    fn labels_have_no_decorative_unicode() {
        for locale in ["en", "pt-BR"] {
            let payload = build_payload(
                &sample(PopupKind::Platinum),
                &Settings::default(),
                locale,
                "dark",
                "#107C10",
            );
            for (key, value) in &payload.labels {
                assert!(
                    !value.contains(['\u{00B7}', '\u{2026}', '\u{2192}', '\u{2605}']),
                    "{locale} {key}: {value}"
                );
            }
        }
    }

    #[test]
    fn middle_positions_center_vertically() {
        let place = |p| position_for(p, (0, 0), (1920, 1040), (520, 150), (24, 24));
        assert_eq!(place(Position::MiddleLeft), (24, 445));
        assert_eq!(place(Position::MiddleRight), (1376, 445));
    }

    #[test]
    fn negative_inset_lets_the_bleed_hang_off_screen() {
        let place = |p| position_for(p, (0, 0), (1920, 1080), (520, 160), (8, -12));
        assert_eq!(place(Position::TopLeft), (8, -12));
        assert_eq!(place(Position::BottomRight), (1392, 932));
        assert_eq!(place(Position::TopCenter), (700, -12));
    }

    #[test]
    fn inset_puts_the_card_at_the_margin() {
        assert_eq!(edge_inset(40.0, 1.0, 1.0), 8);
        assert_eq!(edge_inset(133.0, 1.0, 1.5), -128);
        assert_eq!(edge_inset(40.0, 1.5, 1.0), -12);
        assert_eq!(edge_inset(0.0, 1.0, 1.25), 60);
    }

    #[test]
    fn position_for_each_corner() {
        let origin = (0, 0);
        let screen = (1920, 1080);
        let window = (400, 100);
        assert_eq!(
            position_for(Position::TopLeft, origin, screen, window, (24, 24)),
            (24, 24)
        );
        assert_eq!(
            position_for(Position::TopCenter, origin, screen, window, (24, 24)),
            (760, 24)
        );
        assert_eq!(
            position_for(Position::TopRight, origin, screen, window, (24, 24)),
            (1496, 24)
        );
        assert_eq!(
            position_for(Position::BottomLeft, origin, screen, window, (24, 24)),
            (24, 956)
        );
        assert_eq!(
            position_for(Position::BottomCenter, origin, screen, window, (24, 24)),
            (760, 956)
        );
        assert_eq!(
            position_for(Position::BottomRight, origin, screen, window, (24, 24)),
            (1496, 956)
        );
        assert_eq!(
            position_for(
                Position::BottomRight,
                (-1920, 100),
                screen,
                window,
                (24, 24)
            ),
            (-424, 1056)
        );
        assert_eq!(
            position_for(Position::BottomRight, origin, (300, 50), window, (24, 24)),
            (-124, -74)
        );
    }

    #[test]
    fn payload_json_is_escaped() {
        let mut request = sample(PopupKind::Rare);
        request.display_name = "A \"quoted\" </script> name".into();
        request.description = "line\nbreak".into();
        request.icon_path = DEFAULT_ICON.into();
        let payload = build_payload(&request, &Settings::default(), "en", "dark", "#107C10");
        let script = show_script(&payload);
        assert!(script.starts_with("window.__awShow && window.__awShow({"));
        assert!(script.contains(r#""displayName":"A \"quoted\" </script> name""#));
        assert!(script.contains(r#""description":"line\nbreak""#));
        assert!(script.contains(r#""iconPath":"aw-icon/_default.svg""#));
    }

    #[test]
    fn classify_platinum_then_rare_then_unlock() {
        assert_eq!(classify(Some(50.0), 10.0, true), PopupKind::Platinum);
        assert_eq!(classify(Some(9.9), 10.0, false), PopupKind::Rare);
        assert_eq!(classify(Some(10.0), 10.0, false), PopupKind::Unlock);
        assert_eq!(classify(None, 10.0, false), PopupKind::Unlock);
    }

    #[test]
    fn preset_urls_are_encoded() {
        assert_eq!(
            preset_url("Xbox One"),
            "http://aw.localhost/preset/Xbox%20One/index.html"
        );
        assert_eq!(
            preset_url("aw-minimal"),
            "http://aw.localhost/preset/aw-minimal/index.html"
        );
        assert_eq!(encode_segment("a#b?c"), "a%23b%3Fc");
    }
}
