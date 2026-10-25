use std::collections::HashMap;
use std::sync::OnceLock;

use aw_core::lang::{best_match, display_languages};
use serde::Serialize;

pub const AUTO: &str = "auto";
pub const FALLBACK: &str = "en";
const NAME_KEY: &str = "meta.name";

const LOCALES: [(&str, &str); 2] = [
    ("en", include_str!("../../../ui/src/i18n/locales/en.json")),
    (
        "pt-BR",
        include_str!("../../../ui/src/i18n/locales/pt-BR.json"),
    ),
];

pub mod keys {
    pub const TRAY_OPEN: &str = "tray.open";
    pub const TRAY_TEST: &str = "tray.test";
    pub const TRAY_QUIT: &str = "tray.quit";
    pub const TEST_TITLE: &str = "popup.testTitle";
    pub const TEST_DESCRIPTION: &str = "popup.testDescription";
    pub const FIRST_BOOT_TITLE: &str = "popup.firstBootTitle";
    pub const FIRST_BOOT_DESCRIPTION: &str = "popup.firstBootDescription";
    pub const UNLOCKED: &str = "popup.unlocked";
    pub const RARE: &str = "popup.rare";
    pub const PLATINUM: &str = "popup.platinum";
    pub const PROGRESS: &str = "popup.progress";
}

type Dictionary = HashMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocaleInfo {
    pub code: &'static str,
    pub name: String,
}

fn dictionaries() -> &'static HashMap<&'static str, Dictionary> {
    static DICTIONARIES: OnceLock<HashMap<&'static str, Dictionary>> = OnceLock::new();
    DICTIONARIES.get_or_init(|| {
        LOCALES
            .iter()
            .map(|(code, json)| (*code, serde_json::from_str(json).unwrap_or_default()))
            .collect()
    })
}

pub fn codes() -> Vec<&'static str> {
    LOCALES.iter().map(|(code, _)| *code).collect()
}

pub fn supported() -> Vec<LocaleInfo> {
    codes()
        .into_iter()
        .map(|code| LocaleInfo {
            code,
            name: text(code, NAME_KEY),
        })
        .collect()
}

pub fn is_preference(value: &str) -> bool {
    value == AUTO || codes().iter().any(|code| code.eq_ignore_ascii_case(value))
}

pub fn resolve(preference: &str) -> &'static str {
    resolve_with(preference, &display_languages())
}

pub fn resolve_with(preference: &str, system: &[String]) -> &'static str {
    let supported = codes();
    if preference != AUTO
        && let Some(code) = best_match(&[preference.to_owned()], &supported)
    {
        return code;
    }
    best_match(system, &supported).unwrap_or(FALLBACK)
}

fn lookup(locale: &str, key: &str) -> Option<String> {
    dictionaries().get(locale)?.get(key).cloned()
}

pub fn text(locale: &str, key: &str) -> String {
    lookup(locale, key)
        .or_else(|| lookup(FALLBACK, key))
        .unwrap_or_else(|| key.to_owned())
}

pub fn percent(locale: &str, value: f64) -> String {
    let text = format!("{value:.1}%");
    if locale.starts_with("pt") {
        text.replace('.', ",")
    } else {
        text
    }
}

pub fn text_with(locale: &str, key: &str, vars: &[(&str, String)]) -> String {
    vars.iter().fold(text(locale, key), |out, (name, value)| {
        out.replace(&format!("{{{name}}}"), value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn keys_of(code: &str) -> BTreeSet<String> {
        dictionaries()[code].keys().cloned().collect()
    }

    fn system(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|t| (*t).to_owned()).collect()
    }

    #[test]
    fn every_locale_parses_and_has_every_key() {
        let reference = keys_of(FALLBACK);
        assert!(reference.len() > 100);
        for code in codes() {
            let keys = keys_of(code);
            let missing: Vec<_> = reference.difference(&keys).collect();
            let extra: Vec<_> = keys.difference(&reference).collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "{code}: missing {missing:?}, extra {extra:?}"
            );
        }
    }

    #[test]
    fn every_locale_file_is_registered() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/i18n/locales");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter_map(|e| {
                e.path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .collect();
        files.sort();
        let mut registered: Vec<String> = codes().into_iter().map(str::to_owned).collect();
        registered.sort();
        assert_eq!(files, registered);
    }

    const ALL_KEYS: [&str; 11] = [
        keys::TRAY_OPEN,
        keys::TRAY_TEST,
        keys::TRAY_QUIT,
        keys::TEST_TITLE,
        keys::TEST_DESCRIPTION,
        keys::FIRST_BOOT_TITLE,
        keys::FIRST_BOOT_DESCRIPTION,
        keys::UNLOCKED,
        keys::RARE,
        keys::PLATINUM,
        keys::PROGRESS,
    ];

    #[test]
    fn rust_keys_exist() {
        for key in ALL_KEYS {
            assert!(lookup(FALLBACK, key).is_some(), "{key}");
        }
    }

    #[test]
    fn explicit_preference_wins_over_system() {
        assert_eq!(resolve_with("en", &system(&["pt-BR"])), "en");
        assert_eq!(resolve_with("pt-BR", &system(&["en-US"])), "pt-BR");
    }

    #[test]
    fn auto_uses_closest_display_language() {
        assert_eq!(resolve_with(AUTO, &system(&["pt-PT"])), "pt-BR");
        assert_eq!(resolve_with(AUTO, &system(&["de-DE", "pt-BR"])), "pt-BR");
        assert_eq!(resolve_with(AUTO, &system(&["es-ES"])), "en");
        assert_eq!(resolve_with(AUTO, &[]), "en");
        assert_eq!(resolve_with("klingon", &system(&["pt-BR"])), "pt-BR");
    }

    #[test]
    fn placeholders_and_fallback() {
        assert_eq!(
            text_with(
                "pt-BR",
                keys::FIRST_BOOT_TITLE,
                &[("achievements", "3".into()), ("games", "2".into())]
            ),
            "3 conquistas em 2 jogos"
        );
        assert_eq!(text("xx", keys::TRAY_QUIT), "Quit");
        assert_eq!(text("en", "missing.key"), "missing.key");
        assert_eq!(supported()[1].name, "Português (Brasil)");
    }
}
