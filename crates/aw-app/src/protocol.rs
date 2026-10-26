use std::borrow::Cow;

use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext};

use aw_core::discovery::steam::steam_install_path;

use crate::app::AppState;
use crate::art::{self, ArtKind};
use crate::popup::{PROTOCOL_ORIGIN, PopupKind, SHIM, build_payload};
use crate::presets::{self, Durations, PRESET_CSP, mime_for, percent_decode, resolve_inside};
use crate::settings::Position;

pub const DEFAULT_ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="12" fill="#1d2433"/><path d="M21 14h22v8c0 9-5 15-11 16-6-1-11-7-11-16z" fill="#f2b632"/><path d="M21 17h-6c0 7 3 11 8 12M43 17h6c0 7-3 11-8 12" fill="none" stroke="#f2b632" stroke-width="3" stroke-linecap="round"/><rect x="29" y="38" width="6" height="8" fill="#f2b632"/><rect x="22" y="45" width="20" height="5" rx="2" fill="#f2b632"/></svg>"##;

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    let path = percent_decode(request.uri().path());
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let state = ctx.app_handle().state::<AppState>();
    match segments.as_slice() {
        ["icon", "_default.svg"] => ok(DEFAULT_ICON_SVG.as_bytes().to_vec(), "image/svg+xml", None),
        ["icon", rest @ ..] => {
            let root = state.paths.cache_dir.join("icons");
            serve(resolve_inside(&root, rest), None)
        }
        ["art", app_id, kind] => {
            let file = app_id
                .parse::<u32>()
                .ok()
                .zip(ArtKind::from_name(kind))
                .and_then(|(app_id, kind)| {
                    art::resolve_art(
                        steam_install_path().as_deref(),
                        &state.paths.cache_dir,
                        app_id,
                        kind,
                    )
                });
            serve(file, None)
        }
        ["preset", id, rest @ ..] => {
            let preset = presets::find(&state.paths.builtin_presets, &state.paths.user_presets, id);
            let file = preset.as_ref().and_then(|p| resolve_inside(&p.dir, rest));
            let query = request.uri().query();
            match (preset, file) {
                (Some(preset), Some(file))
                    if wants_preview(query) && mime_for(&file).starts_with("text/html") =>
                {
                    serve_preview(&state, &preset.durations, &file, query)
                }
                (_, file) => serve(file, Some(PRESET_CSP)),
            }
        }
        _ => not_found(),
    }
}

const PREVIEW_PAUSE_MS: u64 = 900;
const PREVIEW_CENTER: &str = "middle-center";

pub fn wants_preview(query: Option<&str>) -> bool {
    query.is_some_and(|q| q.split('&').any(|pair| pair == "preview=loop"))
}

fn query_value<'a>(query: Option<&'a str>, key: &str) -> Option<&'a str> {
    query?
        .split('&')
        .find_map(|pair| pair.strip_prefix(key)?.strip_prefix('='))
}

fn query_enum<T: serde::de::DeserializeOwned>(query: Option<&str>, key: &str) -> Option<T> {
    let value = query_value(query, key)?;
    serde_json::from_value(serde_json::Value::String(value.to_owned())).ok()
}

pub fn preview_kind(query: Option<&str>) -> PopupKind {
    query_enum(query, "kind").unwrap_or(PopupKind::Unlock)
}

pub fn preview_position(query: Option<&str>) -> &'static str {
    query_enum::<Position>(query, "position").map_or(PREVIEW_CENTER, Position::as_str)
}

fn head_end(html: &str) -> Option<usize> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find("<head") {
        let start = from + found;
        let next = lower.as_bytes().get(start + 5).copied();
        if next.is_some_and(|byte| byte == b'>' || byte.is_ascii_whitespace()) {
            return lower[start..].find('>').map(|end| start + end + 1);
        }
        from = start + 5;
    }
    None
}

pub fn with_preview(html: &str, query: Option<&str>, script: &str) -> String {
    if !wants_preview(query) {
        return html.to_owned();
    }
    let tags = format!("<script>{SHIM}</script><script>{script}</script>");
    match head_end(html) {
        Some(at) => format!("{}{tags}{}", &html[..at], &html[at..]),
        None => format!("{tags}{html}"),
    }
}

pub fn loop_script(payload: &str, period_ms: u64) -> String {
    format!(
        "(() => {{ const payload = {payload}; const run = () => window.__awShow && window.__awShow(payload); const start = () => {{ run(); setInterval(run, {period_ms}); }}; if (document.readyState === \"complete\") start(); else addEventListener(\"load\", start); }})();"
    )
}

fn serve_preview(
    state: &AppState,
    durations: &Durations,
    file: &std::path::Path,
    query: Option<&str>,
) -> Response<Cow<'static, [u8]>> {
    let Ok(html) = std::fs::read_to_string(file) else {
        return not_found();
    };
    let settings = state.settings_snapshot();
    let locale = state.ui_locale();
    let kind = preview_kind(query);
    let mut sample = crate::app::test_request(locale, kind, None, true);
    sample.icon_path = format!("{PROTOCOL_ORIGIN}/icon/_default.svg");
    let mut payload = build_payload(
        &sample,
        &settings,
        locale,
        crate::system_theme::effective_theme(settings.theme),
        &crate::system_theme::windows_accent(),
    );
    payload.sound_enabled = false;
    payload.scale = 1.0;
    payload.position = preview_position(query).into();
    let json = serde_json::to_string(&payload)
        .unwrap_or_else(|_| "{}".into())
        .replace("</", "<\\/");
    let page = with_preview(
        &html,
        query,
        &loop_script(&json, durations.for_kind(kind) + PREVIEW_PAUSE_MS),
    );
    ok(page.into_bytes(), mime_for(file), Some(PRESET_CSP))
}

fn serve(
    file: Option<std::path::PathBuf>,
    csp: Option<&'static str>,
) -> Response<Cow<'static, [u8]>> {
    let Some(file) = file else {
        return not_found();
    };
    match std::fs::read(&file) {
        Ok(bytes) => {
            let mime = mime_for(&file);
            let csp = csp.filter(|_| mime.starts_with("text/html"));
            ok(bytes, mime, csp)
        }
        Err(_) => not_found(),
    }
}

fn ok(body: Vec<u8>, mime: &str, csp: Option<&'static str>) -> Response<Cow<'static, [u8]>> {
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
    if let Some(csp) = csp {
        builder = builder.header(header::CONTENT_SECURITY_POLICY, csp);
    }
    builder
        .body(Cow::Owned(body))
        .unwrap_or_else(|_| not_found())
}

fn not_found() -> Response<Cow<'static, [u8]>> {
    let mut response = Response::new(Cow::Borrowed(&b"not found"[..]));
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = "<html><HEAD lang=\"en\"><title>x</title></head><body></body></html>";

    #[test]
    fn preview_loop_script_is_injected_only_with_param() {
        let looped = with_preview(HTML, Some("preview=loop"), "LOOP();");
        assert!(looped.starts_with("<html><HEAD lang=\"en\"><script>"));
        assert!(looped.contains("window.__awShim"));
        assert!(looped.contains("<script>LOOP();</script><title>"));
        assert_eq!(with_preview(HTML, None, "LOOP();"), HTML);
        assert_eq!(with_preview(HTML, Some("preview=once"), "LOOP();"), HTML);
        let bare = with_preview("<div></div>", Some("a=b&preview=loop"), "LOOP();");
        assert!(bare.starts_with("<script>"));
        assert!(bare.ends_with("<div></div>"));
    }

    #[test]
    fn preview_query_picks_kind_and_position() {
        assert_eq!(
            preview_kind(Some("preview=loop&kind=rare")),
            PopupKind::Rare
        );
        assert_eq!(
            preview_kind(Some("kind=platinum&preview=loop")),
            PopupKind::Platinum
        );
        assert_eq!(
            preview_kind(Some("preview=loop&kind=progress")),
            PopupKind::Progress
        );
        assert_eq!(
            preview_kind(Some("preview=loop&kind=nope")),
            PopupKind::Unlock
        );
        assert_eq!(preview_kind(None), PopupKind::Unlock);
        assert_eq!(
            preview_position(Some("preview=loop&position=top-left")),
            "top-left"
        );
        assert_eq!(
            preview_position(Some("preview=loop&position=bottom-center")),
            "bottom-center"
        );
        assert_eq!(
            preview_position(Some("preview=loop&position=evil")),
            "middle-center"
        );
        assert_eq!(preview_position(Some("preview=loop")), "middle-center");
    }

    #[test]
    fn loop_script_repeats_the_sample() {
        let script = loop_script(r#"{"displayName":"A"}"#, 5900);
        assert!(script.contains(r#"{"displayName":"A"}"#));
        assert!(script.contains("5900"));
    }
}
