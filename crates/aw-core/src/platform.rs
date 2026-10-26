use serde::{Deserialize, Serialize, Serializer};

pub const SYNTHETIC_BASE: u32 = 0x8000_0000;
const SYNTHETIC_MASK: u32 = 0x7FFF_FFFF;
const MAX_EXTERNAL_ID_LEN: usize = 64;
const INVALID_EXTERNAL_ID: &str = "invalidappid";
const FNV32_OFFSET: u32 = 0x811c_9dc5;
const FNV32_PRIME: u32 = 0x0100_0193;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameImages {
    pub header: Option<String>,
    pub hero: Option<String>,
    pub logo: Option<String>,
    pub capsule: Option<String>,
}

pub fn synthetic_id(key: &ExternalId) -> u32 {
    let text = format!("{}:{}", key.platform.as_str(), key.id.to_ascii_lowercase());
    let hash = text.bytes().fold(FNV32_OFFSET, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(FNV32_PRIME)
    });
    SYNTHETIC_BASE | (hash & SYNTHETIC_MASK)
}

pub fn next_synthetic(app_id: u32) -> u32 {
    SYNTHETIC_BASE | (app_id.wrapping_add(1) & SYNTHETIC_MASK)
}

pub fn is_synthetic(app_id: u32) -> bool {
    app_id >= SYNTHETIC_BASE
}

pub fn uplay_key(name: &str) -> String {
    let name = name.trim();
    let tail = name
        .to_ascii_lowercase()
        .find("ach_")
        .map(|start| &name[start + 4..])
        .filter(|rest| !rest.is_empty())
        .unwrap_or(name);
    match tail.rsplit_once('_') {
        Some((head, digits))
            if !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
                && head.bytes().any(|b| b.is_ascii_alphabetic()) =>
        {
            digits.to_owned()
        }
        _ => tail.to_owned(),
    }
}

fn alnum_key(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub fn map_name(platform: Platform, raw: &str, schema: &[String]) -> Option<String> {
    if let Some(exact) = schema.iter().find(|name| name.eq_ignore_ascii_case(raw)) {
        return Some(exact.clone());
    }
    let key: fn(&str) -> String = match platform {
        Platform::Steam => return None,
        Platform::Ubisoft => uplay_key,
        Platform::Epic | Platform::Gog => alnum_key,
    };
    let wanted = key(raw);
    if wanted.is_empty() {
        return None;
    }
    let mut matches = schema
        .iter()
        .filter(|name| key(name).eq_ignore_ascii_case(&wanted));
    let first = matches.next()?;
    matches.next().is_none().then(|| first.clone())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Platform {
    Steam,
    Ubisoft,
    Epic,
    Gog,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Steam => "steam",
            Platform::Ubisoft => "uplay",
            Platform::Epic => "epic",
            Platform::Gog => "gog",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Platform::Steam => "Steam",
            Platform::Ubisoft => "Ubisoft",
            Platform::Epic => "Epic",
            Platform::Gog => "GOG",
        }
    }

    pub fn parse(name: &str) -> Option<Platform> {
        [
            Platform::Steam,
            Platform::Ubisoft,
            Platform::Epic,
            Platform::Gog,
        ]
        .into_iter()
        .find(|platform| platform.as_str().eq_ignore_ascii_case(name.trim()))
    }
}

impl Serialize for Platform {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct ExternalId {
    pub platform: Platform,
    pub id: String,
}

pub fn valid_external_id(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_EXTERNAL_ID_LEN
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && !name.bytes().all(|b| b == b'0')
        && !name.eq_ignore_ascii_case(INVALID_EXTERNAL_ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_names() {
        for platform in [
            Platform::Steam,
            Platform::Ubisoft,
            Platform::Epic,
            Platform::Gog,
        ] {
            assert_eq!(Platform::parse(platform.as_str()), Some(platform));
        }
        assert_eq!(Platform::Ubisoft.as_str(), "uplay");
        assert_eq!(Platform::Gog.label(), "GOG");
        assert_eq!(Platform::parse("EPIC"), Some(Platform::Epic));
        assert_eq!(Platform::parse("origin"), None);
    }

    #[test]
    fn serializes_as_short_name() {
        let key = ExternalId {
            platform: Platform::Epic,
            id: "Carnation".into(),
        };
        assert_eq!(
            serde_json::to_string(&key).unwrap(),
            r#"{"platform":"epic","id":"Carnation"}"#
        );
    }

    #[test]
    fn validates_external_ids() {
        for good in [
            "4",
            "Carnation",
            "9d2d0eb64d5c44529cece33fe2a46482",
            "a.b-c_d",
        ] {
            assert!(valid_external_id(good), "{good}");
        }
        let long = "a".repeat(65);
        for bad in [
            "",
            "0000",
            "invalidappid",
            "InvalidAppId",
            "a b",
            "a/b",
            long.as_str(),
        ] {
            assert!(!valid_external_id(bad), "{bad}");
        }
    }

    #[test]
    fn synthetic_id_is_stable_and_high() {
        let a = ExternalId {
            platform: Platform::Epic,
            id: "ABC".into(),
        };
        let b = ExternalId {
            platform: Platform::Epic,
            id: "abc".into(),
        };
        let c = ExternalId {
            platform: Platform::Gog,
            id: "abc".into(),
        };
        assert_eq!(synthetic_id(&a), synthetic_id(&b));
        assert_ne!(synthetic_id(&a), synthetic_id(&c));
        assert!(synthetic_id(&a) >= SYNTHETIC_BASE);
        assert!(is_synthetic(synthetic_id(&c)));
        assert!(!is_synthetic(1_245_620));
    }

    #[test]
    fn game_images_round_trip_as_json() {
        let images = GameImages {
            header: Some("h".into()),
            hero: None,
            logo: Some("l".into()),
            capsule: None,
        };
        let text = serde_json::to_string(&images).unwrap();
        assert_eq!(serde_json::from_str::<GameImages>(&text).unwrap(), images);
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn uplay_key_follows_pserban_rule() {
        for raw in [
            "12",
            "ACH_12",
            "AC2_Ach_12",
            "ACH_ENTER_ANIMUS_12",
            " ach_12 ",
        ] {
            assert_eq!(uplay_key(raw), "12", "{raw}");
        }
        assert_eq!(uplay_key("ACH_WIN"), "WIN");
        assert_eq!(uplay_key("STAGE_2"), "2");
        assert_eq!(uplay_key("_2"), "_2");
        assert_eq!(uplay_key("ACH_"), "ACH_");
    }

    #[test]
    fn map_name_prefers_exact_match() {
        let schema = names(&["ACH_ONE_1", "ach_12"]);
        assert_eq!(
            map_name(Platform::Ubisoft, "ACH_12", &schema).as_deref(),
            Some("ach_12")
        );
    }

    #[test]
    fn map_name_ubisoft_uses_numeric_key_when_unique() {
        let schema = names(&["ACH_ONE_1", "ACH_TWO_2"]);
        assert_eq!(
            map_name(Platform::Ubisoft, "1", &schema).as_deref(),
            Some("ACH_ONE_1")
        );
        let ambiguous = names(&["ACH_ONE_1", "STAGE_1"]);
        assert_eq!(map_name(Platform::Ubisoft, "1", &ambiguous), None);
        assert_eq!(map_name(Platform::Ubisoft, "9", &schema), None);
    }

    #[test]
    fn map_name_epic_and_gog_compare_letters_and_digits() {
        let schema = names(&["ACH_FIRST_BLOOD", "ACH_LAST"]);
        assert_eq!(
            map_name(Platform::Epic, "Ach-First Blood", &schema).as_deref(),
            Some("ACH_FIRST_BLOOD")
        );
        assert_eq!(
            map_name(Platform::Gog, "achlast", &schema).as_deref(),
            Some("ACH_LAST")
        );
        assert_eq!(map_name(Platform::Gog, "none", &schema), None);
        assert_eq!(map_name(Platform::Steam, "ach-last", &schema), None);
    }
}
