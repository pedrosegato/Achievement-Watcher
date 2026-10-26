use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::app::AppState;

pub const OVERLAY_LABEL: &str = "overlay";
pub const OVERLAY_SHOWN: &str = "overlay-shown";
pub const DEFAULT_HOTKEY: &str = "Ctrl+Shift+A";
const WIDTH: f64 = 400.0;
const MAX_HEIGHT: f64 = 760.0;
const MARGIN: f64 = 24.0;

fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(
        app,
        OVERLAY_LABEL,
        WebviewUrl::App("index.html#overlay".into()),
    )
    .title("Achievement Watcher overlay")
    .transparent(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .focusable(false)
    .shadow(false)
    .resizable(false)
    .visible(false)
    .inner_size(WIDTH, 600.0)
    .build()
}

pub fn toggle(app: &AppHandle) {
    let window = match app.get_webview_window(OVERLAY_LABEL) {
        Some(window) => window,
        None => match create(app) {
            Ok(window) => window,
            Err(error) => {
                tracing::warn!("cannot create overlay: {error}");
                return;
            }
        },
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let factor = monitor.scale_factor();
        let area = monitor.work_area();
        let margin = (MARGIN * factor).round() as i32;
        let width = (WIDTH * factor).round() as u32;
        let height = (f64::from(area.size.height) - 2.0 * MARGIN * factor)
            .min(MAX_HEIGHT * factor)
            .max(200.0) as u32;
        let x = area.position.x + i32::try_from(area.size.width).unwrap_or(0)
            - i32::try_from(width).unwrap_or(0)
            - margin;
        let y = area.position.y + margin;
        let _ = window.set_size(PhysicalSize::new(width, height));
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    let _ = app.emit_to(OVERLAY_LABEL, OVERLAY_SHOWN, ());
}

pub fn register_hotkey(app: &AppHandle, accelerator: &str) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    let shortcut: Shortcut = accelerator
        .parse()
        .map_err(|e| format!("invalid shortcut {accelerator}: {e}"))?;
    shortcuts
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle(app);
            }
        })
        .map_err(|e| e.to_string())
}

pub fn apply_hotkey(app: &AppHandle, accelerator: &str) {
    let result = register_hotkey(app, accelerator);
    if let Err(error) = &result {
        tracing::warn!("overlay shortcut: {error}");
    }
    *app.state::<AppState>().hotkey_error() = result.err();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hotkey_parses() {
        assert!(DEFAULT_HOTKEY.parse::<Shortcut>().is_ok());
        assert!("Nope+Key".parse::<Shortcut>().is_err());
    }
}
