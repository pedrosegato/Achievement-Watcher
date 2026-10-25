const TABLE: [(&str, &str); 28] = [
    ("pt-br", "brazilian"),
    ("pt", "portuguese"),
    ("es-es", "spanish"),
    ("es", "latam"),
    ("zh-cn", "schinese"),
    ("zh-sg", "schinese"),
    ("zh-hans", "schinese"),
    ("zh", "tchinese"),
    ("ko", "koreana"),
    ("ja", "japanese"),
    ("fr", "french"),
    ("de", "german"),
    ("it", "italian"),
    ("ru", "russian"),
    ("pl", "polish"),
    ("tr", "turkish"),
    ("uk", "ukrainian"),
    ("cs", "czech"),
    ("hu", "hungarian"),
    ("nl", "dutch"),
    ("da", "danish"),
    ("fi", "finnish"),
    ("nb", "norwegian"),
    ("sv", "swedish"),
    ("th", "thai"),
    ("vi", "vietnamese"),
    ("el", "greek"),
    ("ro", "romanian"),
];
const EXTRA: [(&str, &str); 4] = [
    ("bg", "bulgarian"),
    ("id", "indonesian"),
    ("ar", "arabic"),
    ("no", "norwegian"),
];

pub fn steam_language_for(locale: &str) -> &'static str {
    let locale = locale.trim().to_ascii_lowercase().replace('_', "-");
    let full = TABLE
        .iter()
        .chain(EXTRA.iter())
        .find(|(code, _)| code.contains('-') && locale.starts_with(code));
    if let Some((_, language)) = full {
        return language;
    }
    let primary = locale.split('-').next().unwrap_or_default();
    TABLE
        .iter()
        .chain(EXTRA.iter())
        .find(|(code, _)| *code == primary)
        .map_or("english", |(_, language)| language)
}

pub fn steam_language() -> &'static str {
    steam_language_for(display_languages().first().map_or("", String::as_str))
}

pub fn best_match<'a>(candidates: &[String], supported: &[&'a str]) -> Option<&'a str> {
    let primary = |tag: &str| {
        tag.split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
    };
    candidates.iter().find_map(|candidate| {
        let candidate = candidate.trim().replace('_', "-");
        supported
            .iter()
            .find(|code| code.eq_ignore_ascii_case(&candidate))
            .or_else(|| {
                supported
                    .iter()
                    .find(|code| primary(code) == primary(&candidate))
            })
            .copied()
    })
}

pub fn display_languages() -> Vec<String> {
    let preferred = preferred_ui_languages();
    if preferred.is_empty() {
        user_locale().into_iter().collect()
    } else {
        preferred
    }
}

#[cfg(windows)]
fn preferred_ui_languages() -> Vec<String> {
    use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    use windows::core::PWSTR;

    let mut count = 0u32;
    let mut len = 0u32;
    // SAFETY: size query with no buffer; both out pointers are valid locals.
    if unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut len) }
        .is_err()
        || len == 0
    {
        return Vec::new();
    }
    let mut buffer = vec![0u16; len as usize];
    // SAFETY: `buffer` holds `len` UTF-16 units and outlives the call.
    let filled = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut len,
        )
    };
    if filled.is_err() {
        return Vec::new();
    }
    buffer
        .split(|&unit| unit == 0)
        .filter(|part| !part.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

#[cfg(not(windows))]
fn preferred_ui_languages() -> Vec<String> {
    Vec::new()
}

#[cfg(windows)]
pub fn user_locale() -> Option<String> {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; 85];
    // SAFETY: the buffer is LOCALE_NAME_MAX_LENGTH wide and lives for the call.
    let len = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    let len = usize::try_from(len).ok()?.checked_sub(1)?;
    Some(String::from_utf16_lossy(&buffer[..len]))
}

#[cfg(not(windows))]
pub fn user_locale() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn best_match_prefers_exact_then_same_language() {
        let supported = ["en", "pt-BR"];
        assert_eq!(best_match(&tags(&["pt-BR"]), &supported), Some("pt-BR"));
        assert_eq!(best_match(&tags(&["pt-PT"]), &supported), Some("pt-BR"));
        assert_eq!(best_match(&tags(&["en-GB"]), &supported), Some("en"));
        assert_eq!(
            best_match(&tags(&["de-DE", "pt-BR"]), &supported),
            Some("pt-BR")
        );
        assert_eq!(best_match(&tags(&["es-ES", "fr-FR"]), &supported), None);
        assert_eq!(best_match(&tags(&["PT_br"]), &supported), Some("pt-BR"));
        assert_eq!(best_match(&[], &supported), None);
    }

    #[test]
    fn display_languages_are_not_empty_on_windows() {
        if cfg!(windows) {
            assert!(!display_languages().is_empty());
        }
    }

    #[test]
    fn lang_mapping_table() {
        assert_eq!(steam_language_for("pt-BR"), "brazilian");
        assert_eq!(steam_language_for("pt-PT"), "portuguese");
        assert_eq!(steam_language_for("es-ES"), "spanish");
        assert_eq!(steam_language_for("es-MX"), "latam");
        assert_eq!(steam_language_for("zh-CN"), "schinese");
        assert_eq!(steam_language_for("zh-Hans-CN"), "schinese");
        assert_eq!(steam_language_for("zh-TW"), "tchinese");
        assert_eq!(steam_language_for("ko-KR"), "koreana");
        assert_eq!(steam_language_for("en-US"), "english");
        assert_eq!(steam_language_for("de_DE"), "german");
        assert_eq!(steam_language_for("xx-YY"), "english");
        assert_eq!(steam_language_for(""), "english");
    }

    #[test]
    fn system_language_is_known() {
        assert!(!steam_language().is_empty());
    }
}
