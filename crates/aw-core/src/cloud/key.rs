use serde::Serialize;

use crate::discovery::Found;
use crate::platform::{ExternalId, Platform, is_synthetic};

const STEAM_SOURCE: &str = "steam";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CloudKey {
    Steam(u32),
    External(ExternalId),
}

impl CloudKey {
    pub fn as_folder_key(&self) -> String {
        match self {
            CloudKey::Steam(app_id) => app_id.to_string(),
            CloudKey::External(key) => {
                format!("{}-{}", key.platform.as_str(), key.id.to_ascii_lowercase())
            }
        }
    }

    pub fn parse(text: &str) -> Option<CloudKey> {
        let text = text.trim();
        if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
            return text.parse().ok().map(CloudKey::Steam);
        }
        let (platform, id) = text.split_once('-')?;
        let platform = Platform::parse(platform)?;
        if platform == Platform::Steam || id.is_empty() {
            return None;
        }
        Some(CloudKey::External(ExternalId {
            platform,
            id: id.to_ascii_lowercase(),
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GameKind {
    Emulator,
    SteamOriginal,
}

pub fn cloud_key_for(app_id: u32, found: &[Found]) -> Option<CloudKey> {
    let mine: Vec<&Found> = found.iter().filter(|f| f.app_id == app_id).collect();
    if mine.iter().any(|f| f.external.is_none()) {
        return Some(CloudKey::Steam(app_id));
    }
    if let Some(key) = mine.iter().find_map(|f| f.external.clone()) {
        return Some(CloudKey::External(key));
    }
    (!is_synthetic(app_id)).then_some(CloudKey::Steam(app_id))
}

pub fn game_kind(app_id: u32, found: &[Found], sources: &[String]) -> GameKind {
    let live = found
        .iter()
        .any(|f| f.app_id == app_id && f.source_id != STEAM_SOURCE);
    let history = sources.iter().any(|source| source != STEAM_SOURCE);
    if live || history {
        GameKind::Emulator
    } else {
        GameKind::SteamOriginal
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::parsers::Format;

    fn found(source_id: &'static str, app_id: u32, external: Option<ExternalId>) -> Found {
        Found {
            source_id,
            app_id,
            external,
            path: PathBuf::from("x"),
            format: Format::GoldbergJson,
        }
    }

    #[test]
    fn folder_key_round_trips() {
        let key = CloudKey::External(ExternalId {
            platform: Platform::Ubisoft,
            id: "AbC".into(),
        });
        assert_eq!(key.as_folder_key(), "uplay-abc");
        assert_eq!(
            CloudKey::parse("uplay-abc"),
            Some(CloudKey::External(ExternalId {
                platform: Platform::Ubisoft,
                id: "abc".into(),
            }))
        );
        assert_eq!(CloudKey::parse("374320"), Some(CloudKey::Steam(374320)));
        assert_eq!(CloudKey::parse("banana"), None);
        assert_eq!(CloudKey::parse("steam-12"), None);
    }

    #[test]
    fn steam_found_wins_over_external_for_the_key() {
        let ext = ExternalId {
            platform: Platform::Epic,
            id: "Carnation".into(),
        };
        let list = [
            found("nemirtingas_epic", 7, Some(ext.clone())),
            found("goldberg", 7, None),
        ];
        assert_eq!(cloud_key_for(7, &list), Some(CloudKey::Steam(7)));
        let only_ext = [found("nemirtingas_epic", 7, Some(ext.clone()))];
        assert_eq!(cloud_key_for(7, &only_ext), Some(CloudKey::External(ext)));
    }

    #[test]
    fn synthetic_game_without_found_has_no_key() {
        assert_eq!(cloud_key_for(0x8000_0001, &[]), None);
        assert_eq!(cloud_key_for(374320, &[]), Some(CloudKey::Steam(374320)));
    }

    #[test]
    fn any_emulator_source_makes_the_game_an_emulator_game() {
        let mixed = [found("steam", 9, None), found("goldberg", 9, None)];
        assert_eq!(game_kind(9, &mixed, &[]), GameKind::Emulator);
        let steam = [found("steam", 9, None), found("goldberg", 10, None)];
        assert_eq!(game_kind(9, &steam, &[]), GameKind::SteamOriginal);
    }

    #[test]
    fn emulator_history_in_the_database_counts() {
        let steam_only = ["steam".to_owned()];
        let history = ["steam".to_owned(), "goldberg".to_owned()];
        assert_eq!(game_kind(1245620, &[], &history), GameKind::Emulator);
        assert_eq!(
            game_kind(1245620, &[], &steam_only),
            GameKind::SteamOriginal
        );
    }
}
