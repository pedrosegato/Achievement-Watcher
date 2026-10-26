use crate::discovery::folders::Base;
use crate::parsers::Format;
use crate::platform::Platform;

#[derive(Debug)]
pub struct SourceDef {
    pub id: &'static str,
    pub name: &'static str,
    pub platform: Platform,
    pub base: Base,
    pub root: &'static str,
    pub patterns: &'static [&'static str],
    pub format: Format,
}

static BUILTIN_SOURCES: &[SourceDef] = &[
    SourceDef {
        id: "onlinefix",
        name: "OnlineFix",
        platform: Platform::Steam,
        base: Base::PublicDocuments,
        root: "OnlineFix",
        patterns: &["{appid}/Stats/Achievements.ini", "{appid}/Achievements.ini"],
        format: Format::OnlineFix,
    },
    SourceDef {
        id: "goldberg",
        name: "Goldberg",
        platform: Platform::Steam,
        base: Base::RoamingAppData,
        root: "Goldberg SteamEmu Saves",
        patterns: &["{appid}/achievements.json"],
        format: Format::GoldbergJson,
    },
    SourceDef {
        id: "gse",
        name: "GSE",
        platform: Platform::Steam,
        base: Base::RoamingAppData,
        root: "GSE Saves",
        patterns: &["{appid}/achievements.json"],
        format: Format::GoldbergJson,
    },
    SourceDef {
        id: "codex_public",
        name: "CODEX",
        platform: Platform::Steam,
        base: Base::PublicDocuments,
        root: "Steam/CODEX",
        patterns: &["{appid}/achievements.ini"],
        format: Format::CodexIni,
    },
    SourceDef {
        id: "codex_roaming",
        name: "CODEX",
        platform: Platform::Steam,
        base: Base::RoamingAppData,
        root: "Steam/CODEX",
        patterns: &["{appid}/achievements.ini"],
        format: Format::CodexIni,
    },
    SourceDef {
        id: "rune",
        name: "RUNE",
        platform: Platform::Steam,
        base: Base::PublicDocuments,
        root: "Steam/RUNE",
        patterns: &["{appid}/achievements.ini"],
        format: Format::CodexIni,
    },
    SourceDef {
        id: "empress_roaming",
        name: "EMPRESS",
        platform: Platform::Steam,
        base: Base::RoamingAppData,
        root: "EMPRESS",
        patterns: &[
            "remote/{appid}/achievements.json",
            "{appid}/remote/{appid}/achievements.json",
        ],
        format: Format::GoldbergJson,
    },
    SourceDef {
        id: "empress_public",
        name: "EMPRESS",
        platform: Platform::Steam,
        base: Base::PublicDocuments,
        root: "EMPRESS",
        patterns: &["{appid}/remote/{appid}/achievements.json"],
        format: Format::GoldbergJson,
    },
    SourceDef {
        id: "reloaded_programdata",
        name: "RLD!/3DM",
        platform: Platform::Steam,
        base: Base::ProgramData,
        root: "Steam",
        patterns: &["*/{appid}/stats/achievements.ini"],
        format: Format::ReloadedIni,
    },
    SourceDef {
        id: "reloaded_rld",
        name: "RLD!",
        platform: Platform::Steam,
        base: Base::ProgramData,
        root: "RLD!",
        patterns: &["{appid}/achievements.ini"],
        format: Format::ReloadedIni,
    },
    SourceDef {
        id: "skidrow_documents",
        name: "SKIDROW",
        platform: Platform::Steam,
        base: Base::Documents,
        root: "SKIDROW",
        patterns: &["{appid}/SteamEmu/UserStats/achiev.ini"],
        format: Format::AchievIni,
    },
    SourceDef {
        id: "skidrow_player",
        name: "SKIDROW",
        platform: Platform::Steam,
        base: Base::Documents,
        root: "Player",
        patterns: &["{appid}/SteamEmu/UserStats/achiev.ini"],
        format: Format::AchievIni,
    },
    SourceDef {
        id: "skidrow_local",
        name: "SKIDROW",
        platform: Platform::Steam,
        base: Base::LocalAppData,
        root: "SKIDROW",
        patterns: &["{appid}/SteamEmu/UserStats/achiev.ini"],
        format: Format::AchievIni,
    },
    SourceDef {
        id: "razor1911",
        name: "RAZOR1911",
        platform: Platform::Steam,
        base: Base::RoamingAppData,
        root: ".1911",
        patterns: &["{appid}/achievement"],
        format: Format::Razor1911,
    },
    SourceDef {
        id: "smartsteamemu",
        name: "SmartSteamEmu",
        platform: Platform::Steam,
        base: Base::RoamingAppData,
        root: "SmartSteamEmu",
        patterns: &["{appid}/stats.bin"],
        format: Format::SmartSteamEmu,
    },
    SourceDef {
        id: "creamapi",
        name: "CreamAPI",
        platform: Platform::Steam,
        base: Base::RoamingAppData,
        root: "CreamAPI",
        patterns: &["{appid}/stats/CreamAPI.Achievements.cfg"],
        format: Format::CreamApi,
    },
    SourceDef {
        id: "rune_uplay_documents",
        name: "RUNE Ubisoft",
        platform: Platform::Ubisoft,
        base: Base::Documents,
        root: "RUNE/Ubisoft Connect",
        patterns: &["achievements/*/{appid}/achievements.cfg"],
        format: Format::RuneCfg,
    },
    SourceDef {
        id: "rune_uplay_public",
        name: "RUNE Ubisoft",
        platform: Platform::Ubisoft,
        base: Base::PublicDocuments,
        root: "RUNE/Ubisoft Connect",
        patterns: &["achievements/*/{appid}/achievements.cfg"],
        format: Format::RuneCfg,
    },
    SourceDef {
        id: "goldberg_uplay_json",
        name: "Goldberg UplayEmu",
        platform: Platform::Ubisoft,
        base: Base::RoamingAppData,
        root: "Goldberg UplayEmu Saves",
        patterns: &["{appid}/achievements.json"],
        format: Format::GenericJson,
    },
    SourceDef {
        id: "goldberg_uplay_ini",
        name: "Goldberg UplayEmu",
        platform: Platform::Ubisoft,
        base: Base::RoamingAppData,
        root: "Goldberg UplayEmu Saves",
        patterns: &["{appid}/achievements.ini"],
        format: Format::GenericIni,
    },
    SourceDef {
        id: "r1_uplay_json",
        name: "R1 UplayEmu",
        platform: Platform::Ubisoft,
        base: Base::RoamingAppData,
        root: "R1 UplayEmu Saves",
        patterns: &["{appid}/achievements.json"],
        format: Format::GenericJson,
    },
    SourceDef {
        id: "r1_uplay_ini",
        name: "R1 UplayEmu",
        platform: Platform::Ubisoft,
        base: Base::RoamingAppData,
        root: "R1 UplayEmu Saves",
        patterns: &["{appid}/achievements.ini"],
        format: Format::GenericIni,
    },
    SourceDef {
        id: "anadius",
        name: "anadius LSX",
        platform: Platform::Steam,
        base: Base::LocalAppData,
        root: "anadius/LSX emu/achievement_watcher",
        patterns: &["{appid}/achievements.ini"],
        format: Format::CodexIni,
    },
    SourceDef {
        id: "nemirtingas_epic",
        name: "Nemirtingas Epic",
        platform: Platform::Epic,
        base: Base::RoamingAppData,
        root: "NemirtingasEpicEmu",
        patterns: &["*/{appid}/achievements.json"],
        format: Format::GenericJson,
    },
    SourceDef {
        id: "nemirtingas_galaxy",
        name: "Nemirtingas Galaxy",
        platform: Platform::Gog,
        base: Base::RoamingAppData,
        root: "NemirtingasGalaxyEmu",
        patterns: &["*/{appid}/achievements.json"],
        format: Format::GenericJson,
    },
    SourceDef {
        id: "universelan",
        name: "UniverseLAN",
        platform: Platform::Gog,
        base: Base::LocalAppData,
        root: "UniverseLAN",
        patterns: &["{appid}/*/Achievements.ini"],
        format: Format::GenericIni,
    },
];

pub fn builtin_sources() -> &'static [SourceDef] {
    BUILTIN_SOURCES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_source_ids_are_unique() {
        let mut ids: Vec<&str> = builtin_sources().iter().map(|s| s.id).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn every_pattern_captures_appid_and_ends_in_a_file_name() {
        for s in builtin_sources() {
            for p in s.patterns {
                let segs: Vec<&str> = p.split('/').collect();
                assert!(segs.contains(&"{appid}"), "{}: {p}", s.id);
                let file = segs.last().unwrap();
                assert!(*file != "{appid}" && *file != "*", "{}: {p}", s.id);
            }
        }
    }

    #[test]
    fn roots_and_segments_are_plain_relative_names() {
        fn is_plain(segment: &str) -> bool {
            !segment.is_empty() && !segment.contains([':', '\\']) && segment != ".."
        }
        for s in builtin_sources() {
            for segment in s.root.split('/') {
                assert!(is_plain(segment), "{}: root {:?}", s.id, s.root);
            }
            assert!(
                !s.root.split('/').any(|seg| seg == "*"),
                "{}: root {:?}",
                s.id,
                s.root
            );
            for p in s.patterns {
                for segment in p.split('/') {
                    assert!(is_plain(segment), "{}: pattern {p:?}", s.id);
                }
                assert!(
                    p.split('/').next_back() != Some("{appid}"),
                    "{}: pattern {p:?}",
                    s.id
                );
                assert!(
                    p.split('/').next_back() != Some("*"),
                    "{}: pattern {p:?}",
                    s.id
                );
            }
        }
    }
}
