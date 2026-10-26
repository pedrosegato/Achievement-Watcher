use crate::settings::Theme;

const XBOX_GREEN: &str = "#107C10";

pub fn accent_hex(dword: u32) -> String {
    let [r, g, b, _] = dword.to_le_bytes();
    format!("#{r:02X}{g:02X}{b:02X}")
}

pub fn windows_accent() -> String {
    aw_core::system::accent_color_dword().map_or_else(|| XBOX_GREEN.to_owned(), accent_hex)
}

pub fn windows_prefers_light() -> bool {
    aw_core::system::apps_use_light_theme().unwrap_or(false)
}

pub fn effective_theme(setting: Theme) -> &'static str {
    match setting {
        Theme::Light => "light",
        Theme::Dark => "dark",
        Theme::System if windows_prefers_light() => "light",
        Theme::System => "dark",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_from_abgr_dword() {
        assert_eq!(accent_hex(0xFFD4_7800), "#0078D4");
        assert_eq!(accent_hex(0xFF10_7C10), "#107C10");
    }

    #[test]
    fn explicit_theme_wins() {
        assert_eq!(effective_theme(Theme::Dark), "dark");
        assert_eq!(effective_theme(Theme::Light), "light");
    }
}
