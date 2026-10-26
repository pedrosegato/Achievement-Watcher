use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::popup::PopupKind;

pub const DEFAULT_DURATION_MS: u64 = 7000;
const DEFAULT_WIDTH: u32 = 450;
const DEFAULT_HEIGHT: u32 = 120;
const MAX_BLEED: f64 = 1000.0;
pub const PRESET_CSP: &str = "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; media-src 'self' data: blob:; connect-src 'self'";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    pub id: String,
    pub name: String,
    pub author: Option<String>,
    pub version: Option<String>,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    pub durations: Durations,
    pub builtin: bool,
    pub valid: bool,
    pub warning: Option<String>,
    pub bleed: (f64, f64),
    #[serde(skip)]
    pub dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Durations {
    pub unlock: u64,
    pub rare: u64,
    pub platinum: u64,
    pub progress: u64,
}

impl Durations {
    pub fn all(ms: u64) -> Durations {
        Durations {
            unlock: ms,
            rare: ms,
            platinum: ms,
            progress: ms,
        }
    }

    pub fn for_kind(&self, kind: PopupKind) -> u64 {
        match kind {
            PopupKind::Unlock => self.unlock,
            PopupKind::Rare => self.rare,
            PopupKind::Platinum => self.platinum,
            PopupKind::Progress => self.progress,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct DurationsJson {
    unlock: Option<f64>,
    rare: Option<f64>,
    platinum: Option<f64>,
    progress: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct PresetJson {
    name: Option<String>,
    author: Option<String>,
    version: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<f64>,
    durations: Option<DurationsJson>,
    bleed: Option<[f64; 2]>,
}

pub fn catalog(builtin_dir: &Path, user_dir: &Path) -> Vec<PresetInfo> {
    let mut by_id: BTreeMap<String, PresetInfo> = BTreeMap::new();
    for (dir, builtin) in [(builtin_dir, true), (user_dir, false)] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                let info = read_preset(&entry.path(), builtin);
                by_id.insert(info.id.to_lowercase(), info);
            }
        }
    }
    let mut list: Vec<PresetInfo> = by_id.into_values().collect();
    list.sort_by_key(|p| (!p.builtin, p.name.to_lowercase()));
    list
}

pub fn find(builtin_dir: &Path, user_dir: &Path, id: &str) -> Option<PresetInfo> {
    catalog(builtin_dir, user_dir)
        .into_iter()
        .find(|p| p.valid && p.id.eq_ignore_ascii_case(id))
}

pub fn read_preset(dir: &Path, builtin: bool) -> PresetInfo {
    let id = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut info = PresetInfo {
        id: id.clone(),
        name: id,
        author: None,
        version: None,
        width: DEFAULT_WIDTH,
        height: DEFAULT_HEIGHT,
        duration_ms: DEFAULT_DURATION_MS,
        durations: Durations::all(DEFAULT_DURATION_MS),
        builtin,
        valid: true,
        warning: None,
        bleed: (0.0, 0.0),
        dir: dir.to_path_buf(),
    };
    let index = dir.join("index.html");
    if !index.is_file() {
        info.valid = false;
        info.warning = Some("missing index.html".into());
        return info;
    }
    let mut per_kind: Option<DurationsJson> = None;
    match std::fs::read(dir.join("preset.json")) {
        Ok(bytes) => match serde_json::from_slice::<PresetJson>(&bytes) {
            Ok(json) => {
                if let Some(name) = json.name.filter(|n| !n.trim().is_empty()) {
                    info.name = name;
                }
                info.author = json.author;
                info.version = json.version;
                info.width = json.width.unwrap_or(info.width);
                info.height = json.height.unwrap_or(info.height);
                if let Some(seconds) = json.duration.filter(|s| s.is_finite() && *s > 0.0) {
                    info.duration_ms = (seconds * 1000.0).round() as u64;
                }
                per_kind = json.durations;
                if let Some([x, y]) = json.bleed
                    && [x, y]
                        .iter()
                        .all(|v| v.is_finite() && (0.0..=MAX_BLEED).contains(v))
                {
                    info.bleed = (x, y);
                }
            }
            Err(error) => info.warning = Some(format!("invalid preset.json: {error}")),
        },
        Err(_) => {
            let html = std::fs::read(&index)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            apply_meta_tags(&html, &mut info);
        }
    }
    info.width = info.width.clamp(50, 4000);
    info.height = info.height.clamp(20, 4000);
    info.duration_ms = info.duration_ms.clamp(1000, 60_000);
    let ms = |seconds: Option<f64>| {
        seconds
            .filter(|s| s.is_finite() && *s > 0.0)
            .map_or(info.duration_ms, |s| {
                ((s * 1000.0).round() as u64).clamp(1000, 60_000)
            })
    };
    info.durations = per_kind.map_or(Durations::all(info.duration_ms), |d| Durations {
        unlock: ms(d.unlock),
        rare: ms(d.rare),
        platinum: ms(d.platinum),
        progress: ms(d.progress),
    });
    info
}

fn apply_meta_tags(html: &str, info: &mut PresetInfo) {
    for attrs in meta_tags(html) {
        let get = |key: &str| {
            attrs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v.trim())
        };
        let number = |v: Option<&str>| v.and_then(|v| v.parse::<f64>().ok()).filter(|n| *n > 0.0);
        match get("name").map(str::to_ascii_lowercase).as_deref() {
            Some("duration") => {
                if let Some(ms) = number(get("content")) {
                    info.duration_ms = ms.round() as u64;
                }
            }
            Some("width") => {
                if let Some(px) = number(get("content")) {
                    info.width = px.round() as u32;
                }
            }
            Some("height") => {
                if let Some(px) = number(get("content")) {
                    info.height = px.round() as u32;
                }
            }
            _ => {
                if let (Some(w), Some(h)) = (number(get("width")), number(get("height"))) {
                    info.width = w.round() as u32;
                    info.height = h.round() as u32;
                }
            }
        }
    }
}

fn meta_tags(html: &str) -> Vec<Vec<(String, String)>> {
    let lower = html.to_ascii_lowercase();
    let mut tags = Vec::new();
    let mut from = 0;
    while let Some(start) = lower[from..].find("<meta") {
        let begin = from + start + 5;
        let Some(len) = lower[begin..].find('>') else {
            break;
        };
        tags.push(parse_attributes(&html[begin..begin + len]));
        from = begin + len;
    }
    tags
}

fn parse_attributes(text: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut rest = text.trim_end_matches('/').trim();
    while !rest.is_empty() {
        let key_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let key = rest[..key_end].trim_matches('/').to_owned();
        rest = rest[key_end..].trim_start();
        let value = if let Some(after) = rest.strip_prefix('=') {
            let after = after.trim_start();
            let (value, remaining) = match after.chars().next() {
                Some(quote @ ('"' | '\'')) => {
                    let body = &after[1..];
                    let end = body.find(quote).unwrap_or(body.len());
                    (&body[..end], body.get(end + 1..).unwrap_or(""))
                }
                _ => {
                    let end = after.find(char::is_whitespace).unwrap_or(after.len());
                    (&after[..end], &after[end..])
                }
            };
            rest = remaining.trim_start();
            value.to_owned()
        } else {
            String::new()
        };
        if !key.is_empty() {
            attrs.push((key, value));
        }
    }
    attrs
}

pub fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(byte) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn resolve_inside(root: &Path, segments: &[&str]) -> Option<PathBuf> {
    if segments
        .iter()
        .any(|s| s.is_empty() || *s == "." || *s == ".." || s.contains(['\\', ':']))
    {
        return None;
    }
    let candidate = segments.iter().fold(root.to_path_buf(), |p, s| p.join(s));
    let root_real = std::fs::canonicalize(root).ok()?;
    let real = std::fs::canonicalize(&candidate).ok()?;
    (real.starts_with(&root_real) && real.is_file()).then_some(real)
}

pub fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("ogg") => "audio/ogg",
        Some("webm") => "video/webm",
        Some("mp4") => "video/mp4",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(root: &Path, id: &str, files: &[(&str, &str)]) {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, body) in files {
            std::fs::write(dir.join(name), body).unwrap();
        }
    }

    #[test]
    fn real_builtin_presets_are_the_five() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("presets");
        let mut ids: Vec<String> = catalog(&dir, &dir.join("none"))
            .into_iter()
            .map(|p| p.id)
            .collect();
        ids.sort();
        assert_eq!(
            ids,
            [
                "aw-minimal",
                "playstation-4",
                "playstation-5",
                "xbox",
                "xbox-360"
            ]
        );
    }

    #[test]
    fn catalog_prefers_user_preset() {
        let tmp = tempfile::tempdir().unwrap();
        let builtin = tmp.path().join("builtin");
        let user = tmp.path().join("user");
        preset(
            &builtin,
            "aw-default",
            &[
                ("index.html", "x"),
                ("preset.json", r#"{"name":"Builtin"}"#),
            ],
        );
        preset(
            &user,
            "AW-Default",
            &[("index.html", "x"), ("preset.json", r#"{"name":"Mine"}"#)],
        );
        preset(&user, "other", &[("index.html", "x")]);
        let list = catalog(&builtin, &user);
        let names: Vec<(&str, bool)> = list.iter().map(|p| (p.name.as_str(), p.builtin)).collect();
        assert_eq!(names, vec![("Mine", false), ("other", false)]);
        assert_eq!(find(&builtin, &user, "aw-default").unwrap().name, "Mine");
    }

    #[test]
    fn preset_json_sets_size_and_duration() {
        let tmp = tempfile::tempdir().unwrap();
        preset(
            tmp.path(),
            "p",
            &[
                ("index.html", "x"),
                (
                    "preset.json",
                    r#"{"name":"P","author":"me","version":"1","width":400,"height":100,"duration":5.5}"#,
                ),
            ],
        );
        let info = read_preset(&tmp.path().join("p"), true);
        assert_eq!(
            (info.width, info.height, info.duration_ms),
            (400, 100, 5500)
        );
        assert_eq!(info.author.as_deref(), Some("me"));
        assert!(info.valid);
    }

    #[test]
    fn bleed_comes_from_preset_json() {
        let tmp = tempfile::tempdir().unwrap();
        preset(
            tmp.path(),
            "framed",
            &[
                ("index.html", "<html></html>"),
                ("preset.json", r#"{"bleed":[40,32]}"#),
            ],
        );
        preset(tmp.path(), "bare", &[("index.html", "<html></html>")]);
        preset(
            tmp.path(),
            "broken",
            &[
                ("index.html", "<html></html>"),
                ("preset.json", r#"{"bleed":[-5,9000]}"#),
            ],
        );
        assert_eq!(
            read_preset(&tmp.path().join("framed"), true).bleed,
            (40.0, 32.0)
        );
        assert_eq!(
            read_preset(&tmp.path().join("bare"), true).bleed,
            (0.0, 0.0)
        );
        assert_eq!(
            read_preset(&tmp.path().join("broken"), true).bleed,
            (0.0, 0.0)
        );
    }

    #[test]
    fn bleed_reaches_the_interface() {
        let tmp = tempfile::tempdir().unwrap();
        preset(
            tmp.path(),
            "framed",
            &[
                ("index.html", "<html></html>"),
                ("preset.json", r#"{"bleed":[40,32]}"#),
            ],
        );
        let json = serde_json::to_value(read_preset(&tmp.path().join("framed"), true)).unwrap();
        assert_eq!(json["bleed"], serde_json::json!([40.0, 32.0]));
    }

    #[test]
    fn builtin_pages_anchor_with_their_bleed() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("presets");
        for id in ["aw-minimal", "playstation-4", "playstation-5", "xbox-360"] {
            let info = read_preset(&dir.join(id), true);
            let html = std::fs::read_to_string(dir.join(id).join("index.html")).unwrap();
            let vars = format!(
                "--bleed-x: {}px; --bleed-y: {}px;",
                info.bleed.0, info.bleed.1
            );
            assert!(html.contains(&vars), "{id} must declare {vars}");
            assert!(
                html.contains("stage.dataset.x"),
                "{id} must anchor by position"
            );
        }
        let xbox = read_preset(&dir.join("xbox"), true);
        let html = std::fs::read_to_string(dir.join("xbox").join("index.html")).unwrap();
        assert!(html.contains("const ZOOM = 0.72;"));
        assert!(html.contains("const BLEED = [185, 120];"));
        assert_eq!(
            xbox.bleed,
            ((185.0_f64 * 0.72).round(), (120.0_f64 * 0.72).round())
        );
    }

    #[test]
    fn preset_durations_per_kind() {
        let tmp = tempfile::tempdir().unwrap();
        preset(
            tmp.path(),
            "aw-minimal",
            &[
                ("index.html", "<html></html>"),
                (
                    "preset.json",
                    r#"{"duration":6.95,"durations":{"unlock":6.95,"rare":7.65,"platinum":8.6,"progress":2.9}}"#,
                ),
            ],
        );
        let info = read_preset(&tmp.path().join("aw-minimal"), true);
        assert_eq!(
            info.durations,
            Durations {
                unlock: 6950,
                rare: 7650,
                platinum: 8600,
                progress: 2900
            }
        );
        preset(
            tmp.path(),
            "plain",
            &[
                ("index.html", "<html></html>"),
                ("preset.json", r#"{"duration":6}"#),
            ],
        );
        let plain = read_preset(&tmp.path().join("plain"), true);
        assert_eq!(plain.durations.rare, 6000);
        assert_eq!(plain.durations.progress, 6000);
    }

    #[test]
    fn meta_tags_are_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        preset(
            tmp.path(),
            "Xbox One",
            &[(
                "index.html",
                "<html><head><meta charset=\"UTF-8\" /><meta name=\"duration\" content=\"7500\" /></head><body><meta width=\"500\" height=\"105\" /></body></html>",
            )],
        );
        let info = read_preset(&tmp.path().join("Xbox One"), false);
        assert_eq!(
            (info.width, info.height, info.duration_ms),
            (500, 105, 7500)
        );
        assert_eq!(info.name, "Xbox One");
    }

    #[test]
    fn missing_index_is_invalid_with_warning() {
        let tmp = tempfile::tempdir().unwrap();
        preset(tmp.path(), "broken", &[("style.css", "x")]);
        let info = read_preset(&tmp.path().join("broken"), false);
        assert!(!info.valid);
        assert_eq!(info.warning.as_deref(), Some("missing index.html"));
        assert!(find(tmp.path(), tmp.path(), "broken").is_none());
    }

    #[test]
    fn invalid_preset_json_keeps_defaults_with_warning() {
        let tmp = tempfile::tempdir().unwrap();
        preset(
            tmp.path(),
            "p",
            &[("index.html", "x"), ("preset.json", "{")],
        );
        let info = read_preset(&tmp.path().join("p"), false);
        assert!(info.valid);
        assert!(info.warning.unwrap().starts_with("invalid preset.json"));
        assert_eq!(info.duration_ms, DEFAULT_DURATION_MS);
    }

    #[test]
    fn path_traversal_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        preset(tmp.path(), "p", &[("index.html", "x")]);
        std::fs::write(tmp.path().join("secret.txt"), "s").unwrap();
        let root = tmp.path().join("p");
        assert!(resolve_inside(&root, &["index.html"]).is_some());
        assert!(resolve_inside(&root, &["..", "secret.txt"]).is_none());
        assert!(resolve_inside(&root, &["..\\secret.txt"]).is_none());
        assert!(resolve_inside(&root, &["C:", "x"]).is_none());
        assert!(resolve_inside(&root, &["missing.css"]).is_none());
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("Xbox%20One"), "Xbox One");
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(percent_decode("%C3%A9"), "é");
    }

    #[test]
    fn builtin_presets_are_valid() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("presets");
        let list = catalog(&dir, &dir.join("none"));
        assert_eq!(list.len(), 5);
        for preset in &list {
            assert!(preset.valid, "{}", preset.id);
            assert!(
                preset.warning.is_none(),
                "{}: {:?}",
                preset.id,
                preset.warning
            );
        }
    }

    #[test]
    fn mime_types() {
        assert_eq!(mime_for(Path::new("a.HTML")), "text/html; charset=utf-8");
        assert_eq!(mime_for(Path::new("a.woff2")), "font/woff2");
        assert_eq!(mime_for(Path::new("a")), "application/octet-stream");
    }
}
