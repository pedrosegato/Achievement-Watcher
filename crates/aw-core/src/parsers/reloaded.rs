use super::epoch_time;
use crate::model::{Progress, Unlock};
use crate::parsers::ini::{Ini, Section};

const NON_ACHIEVEMENT_SECTIONS: [&str; 3] = ["Steam", "Steam64", "SteamAchievements"];
const ACHIEVEMENT_KEYS: [&str; 4] = ["State", "CurProgress", "MaxProgress", "Time"];
const THREEDM_UNLOCKED: &str = "0101";

pub(crate) fn parse_reloaded(text: &str) -> Vec<Unlock> {
    let ini = Ini::parse(text);
    match (ini.section("State"), ini.section("Time")) {
        (Some(state), time) if time.is_some() || !has_achievement_keys(state) => {
            read_threedm(state, time)
        }
        _ => read_rld(&ini),
    }
}

fn has_achievement_keys(section: &Section) -> bool {
    ACHIEVEMENT_KEYS
        .iter()
        .any(|key| section.get(key).is_some())
}

pub(crate) fn hex_le_u32(raw: &str) -> Option<u32> {
    let digits = raw.trim();
    if digits.len() != 8 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    Some(u32::from_le_bytes(value.to_be_bytes()))
}

fn read_threedm(state: &Section, time: Option<&Section>) -> Vec<Unlock> {
    let times = time.map(Section::lookup).unwrap_or_default();
    state
        .entries
        .iter()
        .map(|(name, value)| {
            Unlock::new(
                name.clone(),
                value.trim() == THREEDM_UNLOCKED,
                read_time(times.get(&name.to_ascii_lowercase()).copied()),
            )
        })
        .collect()
}

fn read_rld(ini: &Ini) -> Vec<Unlock> {
    ini.sections()
        .iter()
        .filter(|section| !section.name.is_empty())
        .filter(|section| {
            !NON_ACHIEVEMENT_SECTIONS
                .iter()
                .any(|name| section.name.eq_ignore_ascii_case(name))
        })
        .filter(|section| has_achievement_keys(section))
        .map(read_achievement)
        .collect()
}

fn read_achievement(section: &Section) -> Unlock {
    let flagged = section.get("State").and_then(hex_le_u32) == Some(1);
    let progress = read_progress(section);
    let complete = progress
        .as_ref()
        .is_some_and(|progress| progress.current == progress.max);
    let mut unlock = Unlock::new(
        section.name.clone(),
        flagged || complete,
        read_time(section.get("Time")),
    );
    unlock.progress = progress;
    unlock
}

fn read_progress(section: &Section) -> Option<Progress> {
    let max = section
        .get("MaxProgress")
        .and_then(hex_le_u32)
        .filter(|&max| max > 0)?;
    Some(Progress {
        current: section.get("CurProgress").and_then(hex_le_u32).unwrap_or(0),
        max,
    })
}

fn read_time(raw: Option<&str>) -> Option<i64> {
    raw.and_then(hex_le_u32)
        .and_then(|seconds| epoch_time(i64::from(seconds)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Progress, Unlock};

    #[test]
    fn hex_le_u32_decodes_little_endian() {
        assert_eq!(hex_le_u32("01000000"), Some(1));
        assert_eq!(hex_le_u32("00f15365"), Some(1_700_000_000));
        assert_eq!(hex_le_u32(" 0A000000 "), Some(10));
    }

    #[test]
    fn lookup_is_case_insensitive_across_sections() {
        assert_eq!(
            parse_reloaded(
                "[State]
Abc=0101
[Time]
ABC=00f15365
"
            ),
            vec![Unlock::new("Abc", true, Some(1_700_000_000))]
        );
    }

    #[test]
    fn hex_le_u32_rejects_malformed_values() {
        for bad in ["", "0100", "010000000", "zz000000", "0x000001", "+1000000"] {
            assert_eq!(hex_le_u32(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn reads_rld_sections_with_progress() {
        let u = parse_reloaded(
            "[Steam]\nAppID=0\n[A1]\nState=01000000\nCurProgress=00000000\nMaxProgress=00000000\nTime=00f15365\n[A2]\nState=00000000\nCurProgress=03000000\nMaxProgress=0a000000\nTime=00000000\n",
        );
        let mut a2 = Unlock::new("A2", false, None);
        a2.progress = Some(Progress {
            current: 3,
            max: 10,
        });
        assert_eq!(u, vec![Unlock::new("A1", true, Some(1_700_000_000)), a2]);
    }

    #[test]
    fn rld_full_progress_counts_as_unlocked() {
        let u = parse_reloaded(
            "[G]\nState=00000000\nCurProgress=05000000\nMaxProgress=05000000\nTime=00000000\n",
        );
        let mut g = Unlock::new("G", true, None);
        g.progress = Some(Progress { current: 5, max: 5 });
        assert_eq!(u, vec![g]);
    }

    #[test]
    fn rld_bad_state_hex_is_locked() {
        let u = parse_reloaded("[A]\nState=1\nTime=00f15365\n");
        assert_eq!(u, vec![Unlock::new("A", false, Some(1_700_000_000))]);
    }

    #[test]
    fn reads_3dm_state_and_time_sections() {
        let u = parse_reloaded("[State]\nA1=0101\nA2=0000\n[Time]\nA1=00f15365\nA2=00000000\n");
        assert_eq!(
            u,
            vec![
                Unlock::new("A1", true, Some(1_700_000_000)),
                Unlock::new("A2", false, None)
            ]
        );
    }

    #[test]
    fn reads_threedm_state_without_time_section() {
        let u = parse_reloaded("[State]\nA1=0101\nA2=0000\n");
        assert_eq!(
            u,
            vec![
                Unlock::new("A1", true, None),
                Unlock::new("A2", false, None)
            ]
        );
    }

    #[test]
    fn threedm_with_time_section_stays_threedm_even_with_rld_key_names() {
        let u = parse_reloaded("[State]\nTime=0101\n[Time]\nTime=00f15365\n");
        assert_eq!(u, vec![Unlock::new("Time", true, Some(1_700_000_000))]);
    }

    #[test]
    fn rld_achievement_named_state_stays_rld() {
        let u = parse_reloaded("[State]\nState=01000000\nTime=00f15365\n");
        assert_eq!(u, vec![Unlock::new("State", true, Some(1_700_000_000))]);
    }

    #[test]
    fn threedm_missing_or_bad_time_has_no_time() {
        let u = parse_reloaded("[State]\nA1=0101\n[Time]\nA1=nothex!\n");
        assert_eq!(u, vec![Unlock::new("A1", true, None)]);
    }
}
