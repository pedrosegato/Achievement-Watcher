use crate::discovery::folders::Base;
use crate::parsers::Format;

#[derive(Debug)]
pub struct SourceDef {
    pub id: &'static str,
    pub name: &'static str,
    pub base: Base,
    pub root: &'static str,
    pub patterns: &'static [&'static str],
    pub format: Format,
}

static BUILTIN_SOURCES: &[SourceDef] = &[
    SourceDef {
        id: "onlinefix",
        name: "OnlineFix",
        base: Base::PublicDocuments,
        root: "OnlineFix",
        patterns: &["{appid}/Stats/Achievements.ini", "{appid}/Achievements.ini"],
        format: Format::OnlineFix,
    },
    SourceDef {
        id: "goldberg",
        name: "Goldberg",
        base: Base::RoamingAppData,
        root: "Goldberg SteamEmu Saves",
        patterns: &["{appid}/achievements.json"],
        format: Format::GoldbergJson,
    },
    SourceDef {
        id: "gse",
        name: "GSE",
        base: Base::RoamingAppData,
        root: "GSE Saves",
        patterns: &["{appid}/achievements.json"],
        format: Format::GoldbergJson,
    },
    SourceDef {
        id: "codex_public",
        name: "CODEX",
        base: Base::PublicDocuments,
        root: "Steam/CODEX",
        patterns: &["{appid}/achievements.ini"],
        format: Format::CodexIni,
    },
    SourceDef {
        id: "codex_roaming",
        name: "CODEX",
        base: Base::RoamingAppData,
        root: "Steam/CODEX",
        patterns: &["{appid}/achievements.ini"],
        format: Format::CodexIni,
    },
    SourceDef {
        id: "rune",
        name: "RUNE",
        base: Base::PublicDocuments,
        root: "Steam/RUNE",
        patterns: &["{appid}/achievements.ini"],
        format: Format::CodexIni,
    },
    SourceDef {
        id: "empress_roaming",
        name: "EMPRESS",
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
        base: Base::PublicDocuments,
        root: "EMPRESS",
        patterns: &["{appid}/remote/{appid}/achievements.json"],
        format: Format::GoldbergJson,
    },
    SourceDef {
        id: "reloaded_programdata",
        name: "RLD!/3DM",
        base: Base::ProgramData,
        root: "Steam",
        patterns: &["*/{appid}/stats/achievements.ini"],
        format: Format::ReloadedIni,
    },
    SourceDef {
        id: "reloaded_rld",
        name: "RLD!",
        base: Base::ProgramData,
        root: "RLD!",
        patterns: &["{appid}/achievements.ini"],
        format: Format::ReloadedIni,
    },
    SourceDef {
        id: "skidrow_documents",
        name: "SKIDROW",
        base: Base::Documents,
        root: "SKIDROW",
        patterns: &["{appid}/SteamEmu/UserStats/achiev.ini"],
        format: Format::AchievIni,
    },
    SourceDef {
        id: "skidrow_player",
        name: "SKIDROW",
        base: Base::Documents,
        root: "Player",
        patterns: &["{appid}/SteamEmu/UserStats/achiev.ini"],
        format: Format::AchievIni,
    },
    SourceDef {
        id: "skidrow_local",
        name: "SKIDROW",
        base: Base::LocalAppData,
        root: "SKIDROW",
        patterns: &["{appid}/SteamEmu/UserStats/achiev.ini"],
        format: Format::AchievIni,
    },
    SourceDef {
        id: "razor1911",
        name: "RAZOR1911",
        base: Base::RoamingAppData,
        root: ".1911",
        patterns: &["{appid}/achievement"],
        format: Format::Razor1911,
    },
    SourceDef {
        id: "smartsteamemu",
        name: "SmartSteamEmu",
        base: Base::RoamingAppData,
        root: "SmartSteamEmu",
        patterns: &["{appid}/stats.bin"],
        format: Format::SmartSteamEmu,
    },
    SourceDef {
        id: "creamapi",
        name: "CreamAPI",
        base: Base::RoamingAppData,
        root: "CreamAPI",
        patterns: &["{appid}/stats/CreamAPI.Achievements.cfg"],
        format: Format::CreamApi,
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
