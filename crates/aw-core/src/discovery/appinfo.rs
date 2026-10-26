use std::path::Path;

use serde::Serialize;

pub const MAGIC: u32 = 0x0756_4429;
const HEADER: usize = 16;
const ENTRY_HEADER: usize = 60;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Pin {
    BottomLeft,
    BottomCenter,
    CenterCenter,
    UpperCenter,
    UpperLeft,
}

impl Pin {
    fn parse(text: &str) -> Option<Pin> {
        match text {
            "BottomLeft" => Some(Pin::BottomLeft),
            "BottomCenter" => Some(Pin::BottomCenter),
            "CenterCenter" => Some(Pin::CenterCenter),
            "UpperCenter" => Some(Pin::UpperCenter),
            "UpperLeft" => Some(Pin::UpperLeft),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoPosition {
    pub pin: Pin,
    pub width_pct: f32,
    pub height_pct: f32,
}

enum Value {
    Obj(Vec<(String, Value)>),
    Str(String),
    Int(i64),
}

impl Value {
    fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn as_f32(&self) -> Option<f32> {
        match self {
            Value::Str(text) => text.trim().parse().ok(),
            #[allow(clippy::cast_precision_loss)]
            Value::Int(n) => Some(*n as f32),
            Value::Obj(_) => None,
        }
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn i64_at(bytes: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_le_bytes(
        bytes.get(at..at.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn string_table(bytes: &[u8]) -> Option<Vec<String>> {
    let offset = usize::try_from(i64_at(bytes, 8)?).ok()?;
    let count = u32_at(bytes, offset)?;
    let mut strings = Vec::new();
    let mut at = offset + 4;
    for _ in 0..count {
        let rest = bytes.get(at..)?;
        let end = rest.iter().position(|b| *b == 0)?;
        strings.push(String::from_utf8_lossy(&rest[..end]).into_owned());
        at += end + 1;
    }
    Some(strings)
}

fn read_obj(
    bytes: &[u8],
    at: &mut usize,
    strings: &[String],
    depth: usize,
) -> Option<Vec<(String, Value)>> {
    if depth > MAX_DEPTH {
        return None;
    }
    let mut entries = Vec::new();
    loop {
        let tag = *bytes.get(*at)?;
        *at += 1;
        if tag == 0x08 {
            return Some(entries);
        }
        let key = strings
            .get(usize::try_from(u32_at(bytes, *at)?).ok()?)?
            .clone();
        *at += 4;
        let value = match tag {
            0x00 => Value::Obj(read_obj(bytes, at, strings, depth + 1)?),
            0x01 => {
                let rest = bytes.get(*at..)?;
                let end = rest.iter().position(|b| *b == 0)?;
                let text = String::from_utf8_lossy(&rest[..end]).into_owned();
                *at += end + 1;
                Value::Str(text)
            }
            0x02 => {
                let n = u32_at(bytes, *at)? as i32;
                *at += 4;
                Value::Int(i64::from(n))
            }
            0x07 => {
                let n = i64_at(bytes, *at)?;
                *at += 8;
                Value::Int(n)
            }
            _ => return None,
        };
        entries.push((key, value));
    }
}

fn logo_from(root: &Value) -> Option<LogoPosition> {
    let common = root.get("appinfo")?.get("common")?;
    let logo = ["library_assets_full", "library_assets"]
        .iter()
        .find_map(|key| common.get(key)?.get("logo_position"))?;
    let pin = match logo.get("pinned_position")? {
        Value::Str(text) => Pin::parse(text)?,
        _ => return None,
    };
    Some(LogoPosition {
        pin,
        width_pct: logo.get("width_pct")?.as_f32()?,
        height_pct: logo.get("height_pct")?.as_f32()?,
    })
}

pub fn parse_logo_position(bytes: &[u8], app_id: u32) -> Option<LogoPosition> {
    if u32_at(bytes, 0)? != MAGIC {
        return None;
    }
    let strings = string_table(bytes)?;
    let mut at = HEADER;
    loop {
        let id = u32_at(bytes, at)?;
        if id == 0 {
            return None;
        }
        let size = usize::try_from(u32_at(bytes, at + 4)?).ok()?;
        if id == app_id {
            let mut cursor = at + 8 + ENTRY_HEADER;
            let root = Value::Obj(read_obj(bytes, &mut cursor, &strings, 0)?);
            return logo_from(&root);
        }
        at = at.checked_add(8)?.checked_add(size)?;
    }
}

pub fn read_logo_position(steam: &Path, app_id: u32) -> Option<LogoPosition> {
    let bytes = std::fs::read(steam.join("appcache").join("appinfo.vdf")).ok()?;
    parse_logo_position(&bytes, app_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    enum Node {
        Obj(Vec<(&'static str, Node)>),
        Str(&'static str),
        Int(i32),
    }

    struct Strings(Vec<&'static str>);

    impl Strings {
        fn index(&mut self, key: &'static str) -> u32 {
            if let Some(i) = self.0.iter().position(|s| *s == key) {
                return i as u32;
            }
            self.0.push(key);
            (self.0.len() - 1) as u32
        }
    }

    fn write(entries: &[(&'static str, Node)], strings: &mut Strings, out: &mut Vec<u8>) {
        for (key, node) in entries {
            let tag = match node {
                Node::Obj(_) => 0x00u8,
                Node::Str(_) => 0x01,
                Node::Int(_) => 0x02,
            };
            out.push(tag);
            out.extend_from_slice(&strings.index(key).to_le_bytes());
            match node {
                Node::Obj(children) => {
                    write(children, strings, out);
                    out.push(0x08);
                }
                Node::Str(text) => {
                    out.extend_from_slice(text.as_bytes());
                    out.push(0);
                }
                Node::Int(value) => out.extend_from_slice(&value.to_le_bytes()),
            }
        }
    }

    fn appinfo(magic: u32, apps: Vec<(u32, Node)>) -> Vec<u8> {
        let mut strings = Strings(Vec::new());
        let mut body = Vec::new();
        for (app_id, node) in apps {
            let mut kv = Vec::new();
            write(&[("appinfo", node)], &mut strings, &mut kv);
            kv.push(0x08);
            body.extend_from_slice(&app_id.to_le_bytes());
            body.extend_from_slice(&((60 + kv.len()) as u32).to_le_bytes());
            body.extend_from_slice(&[0u8; 60]);
            body.extend_from_slice(&kv);
        }
        body.extend_from_slice(&0u32.to_le_bytes());
        let mut out = Vec::new();
        out.extend_from_slice(&magic.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&((16 + body.len()) as i64).to_le_bytes());
        out.extend_from_slice(&body);
        out.extend_from_slice(&(strings.0.len() as u32).to_le_bytes());
        for s in strings.0 {
            out.extend_from_slice(s.as_bytes());
            out.push(0);
        }
        out
    }

    fn app(assets_key: &'static str, pin: &'static str, width: Node, height: Node) -> Node {
        Node::Obj(vec![(
            "common",
            Node::Obj(vec![
                ("name", Node::Str("Game")),
                (
                    assets_key,
                    Node::Obj(vec![(
                        "logo_position",
                        Node::Obj(vec![
                            ("pinned_position", Node::Str(pin)),
                            ("width_pct", width),
                            ("height_pct", height),
                        ]),
                    )]),
                ),
            ]),
        )])
    }

    #[test]
    fn reads_bottom_center_with_string_percentages() {
        let bytes = appinfo(
            MAGIC,
            vec![
                (
                    10,
                    app("library_assets", "BottomLeft", Node::Int(1), Node::Int(1)),
                ),
                (
                    413150,
                    app(
                        "library_assets_full",
                        "BottomCenter",
                        Node::Str("77.24"),
                        Node::Str("94.68"),
                    ),
                ),
            ],
        );
        assert_eq!(
            parse_logo_position(&bytes, 413150),
            Some(LogoPosition {
                pin: Pin::BottomCenter,
                width_pct: 77.24,
                height_pct: 94.68
            })
        );
    }

    #[test]
    fn reads_integer_percentages() {
        let bytes = appinfo(
            MAGIC,
            vec![(
                374320,
                app("library_assets", "BottomLeft", Node::Int(54), Node::Int(48)),
            )],
        );
        assert_eq!(
            parse_logo_position(&bytes, 374320),
            Some(LogoPosition {
                pin: Pin::BottomLeft,
                width_pct: 54.0,
                height_pct: 48.0
            })
        );
    }

    #[test]
    fn missing_app_is_none() {
        let bytes = appinfo(
            MAGIC,
            vec![(
                10,
                app(
                    "library_assets",
                    "CenterCenter",
                    Node::Int(30),
                    Node::Int(41),
                ),
            )],
        );
        assert_eq!(parse_logo_position(&bytes, 11), None);
    }

    #[test]
    fn unknown_magic_is_empty() {
        let bytes = appinfo(
            0x0756_4428,
            vec![(
                10,
                app(
                    "library_assets",
                    "CenterCenter",
                    Node::Int(30),
                    Node::Int(41),
                ),
            )],
        );
        assert_eq!(parse_logo_position(&bytes, 10), None);
    }

    #[test]
    fn truncated_file_is_none() {
        let bytes = appinfo(
            MAGIC,
            vec![(
                10,
                app(
                    "library_assets",
                    "UpperCenter",
                    Node::Int(30),
                    Node::Int(41),
                ),
            )],
        );
        assert_eq!(parse_logo_position(&bytes[..40], 10), None);
        assert_eq!(parse_logo_position(&[], 10), None);
    }

    #[test]
    fn pin_serializes_kebab_case() {
        let json = serde_json::to_string(&LogoPosition {
            pin: Pin::CenterCenter,
            width_pct: 1.0,
            height_pct: 2.0,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"pin":"center-center","widthPct":1.0,"heightPct":2.0}"#
        );
    }
}
