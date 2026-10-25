mod achiev;
pub mod binkv;
mod codex;
mod creamapi;
mod generic_ini;
mod goldberg;
pub mod ini;
mod onlinefix;
mod razor;
mod reloaded;
pub mod smartsteam;
pub mod steamstats;
mod tenoke;
pub mod text;
pub mod vdf;

use std::path::Path;

use crate::model::{Unlock, normalize_epoch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    OnlineFix,
    GoldbergJson,
    CodexIni,
    ReloadedIni,
    AchievIni,
    CreamApi,
    TenokeUserStats,
    GenericIni,
    Razor1911,
    SmartSteamEmu,
    SteamStats,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("empty file")]
    Empty,
    #[error("invalid json: {0}")]
    Json(String),
    #[error("{0}")]
    Invalid(String),
}

pub fn parse_file(format: Format, path: &Path, bytes: &[u8]) -> Result<Vec<Unlock>, ParseError> {
    if format == Format::SteamStats {
        let schema = steamstats::schema_path(path)
            .and_then(|p| std::fs::read(p).ok())
            .ok_or_else(|| ParseError::Invalid("steam schema file not found".into()))?;
        return steamstats::parse_steam_stats(bytes, &schema);
    }
    parse(format, bytes)
}

pub fn parse(format: Format, bytes: &[u8]) -> Result<Vec<Unlock>, ParseError> {
    match format {
        Format::SmartSteamEmu => return smartsteam::parse_smartsteam(bytes),
        Format::SteamStats => {
            return Err(ParseError::Invalid(
                "steam stats need their schema file".into(),
            ));
        }
        _ => {}
    }
    let text = text::decode(bytes);
    if text.trim().is_empty() {
        return Err(ParseError::Empty);
    }
    match format {
        Format::OnlineFix => Ok(onlinefix::parse_onlinefix(&text)),
        Format::GoldbergJson => goldberg::parse_goldberg(&text),
        Format::CodexIni => Ok(codex::parse_codex(&text)),
        Format::ReloadedIni => Ok(reloaded::parse_reloaded(&text)),
        Format::AchievIni => Ok(achiev::parse_achiev(&text)),
        Format::CreamApi => Ok(creamapi::parse_creamapi(&text)),
        Format::TenokeUserStats => Ok(tenoke::parse_tenoke(&text)),
        Format::GenericIni => Ok(generic_ini::parse_generic_ini(&text)),
        Format::Razor1911 => Ok(razor::parse_razor(&text)),
        Format::SmartSteamEmu | Format::SteamStats => Ok(Vec::new()),
    }
}

pub(crate) fn unlock_time(raw: &str) -> Option<i64> {
    raw.trim().parse::<i64>().ok().and_then(epoch_time)
}

pub(crate) fn epoch_time(raw: i64) -> Option<i64> {
    (raw > 0).then(|| normalize_epoch(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_an_error() {
        assert!(matches!(
            parse(Format::OnlineFix, b""),
            Err(ParseError::Empty)
        ));
        assert!(matches!(
            parse(Format::OnlineFix, b" \r\n"),
            Err(ParseError::Empty)
        ));
    }

    #[test]
    fn all_nul_file_is_empty() {
        assert!(matches!(
            parse(Format::OnlineFix, &[0u8; 64]),
            Err(ParseError::Empty)
        ));
    }

    #[test]
    fn nul_padded_onlinefix_keeps_unlock() {
        let mut bytes = b"[ACH1]\nachieved=true\ntimestamp=1761321600\n".to_vec();
        bytes.extend_from_slice(&[0u8; 200]);
        assert_eq!(
            parse(Format::OnlineFix, &bytes).unwrap(),
            vec![Unlock::new("ACH1", true, Some(1_761_321_600))]
        );
    }

    #[test]
    fn unlock_time_rejects_non_numbers_and_non_positive_values() {
        assert_eq!(unlock_time("abc"), None);
        assert_eq!(unlock_time(""), None);
        assert_eq!(unlock_time("0"), None);
        assert_eq!(unlock_time("-5"), None);
    }

    #[test]
    fn unlock_time_normalizes_to_seconds() {
        assert_eq!(unlock_time(" 1761321600 "), Some(1_761_321_600));
        assert_eq!(unlock_time("1761321600000"), Some(1_761_321_600));
    }
}
