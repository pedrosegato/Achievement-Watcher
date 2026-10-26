use super::unlock_time;
use crate::model::{Progress, Unlock};
use crate::parsers::ini::{Ini, Section};

const NON_ACHIEVEMENT_SECTIONS: [&str; 3] = ["SteamAchievements", "Steam", "Steam64"];
const ACHIEVEMENT_KEYS: [&str; 4] = ["Achieved", "CurProgress", "MaxProgress", "UnlockTime"];

pub(crate) fn parse_codex(text: &str) -> Vec<Unlock> {
    Ini::parse(text)
        .sections()
        .iter()
        .filter(|section| !section.name.is_empty())
        .filter(|section| {
            !NON_ACHIEVEMENT_SECTIONS
                .iter()
                .any(|name| section.name.eq_ignore_ascii_case(name))
        })
        .filter(|section| {
            ACHIEVEMENT_KEYS
                .iter()
                .any(|key| section.get(key).is_some())
        })
        .map(read_achievement)
        .collect()
}

fn read_achievement(section: &Section) -> Unlock {
    let flagged = section
        .get("Achieved")
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
    let progress = read_progress(section);
    let complete = progress
        .as_ref()
        .is_some_and(|progress| progress.current == progress.max);
    let mut unlock = Unlock::new(
        section.name.clone(),
        flagged || complete,
        section.get("UnlockTime").and_then(unlock_time),
    );
    unlock.progress = progress;
    unlock
}

fn read_progress(section: &Section) -> Option<Progress> {
    let max = read_count(section, "MaxProgress").filter(|&max| max > 0)?;
    Some(Progress {
        current: read_count(section, "CurProgress").unwrap_or(0),
        max,
    })
}

fn read_count(section: &Section, key: &str) -> Option<u32> {
    section.get(key)?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Progress, Unlock};

    #[test]
    fn reads_unlocked_and_locked_with_progress() {
        let u = parse_codex(
            "[A1]\nAchieved=1\nCurProgress=0\nMaxProgress=0\nUnlockTime=1761321600\n\n[A2]\nAchieved=0\nCurProgress=3\nMaxProgress=10\nUnlockTime=0\n",
        );
        let mut a2 = Unlock::new("A2", false, None);
        a2.progress = Some(Progress {
            current: 3,
            max: 10,
        });
        assert_eq!(u, vec![Unlock::new("A1", true, Some(1_761_321_600)), a2]);
    }

    #[test]
    fn skips_steam_index_sections() {
        let u = parse_codex(
            "[SteamAchievements]\n00000=A1\nCount=1\n[Steam]\nAchieved=1\n[Steam64]\nAchieved=1\n",
        );
        assert!(u.is_empty());
    }

    #[test]
    fn full_progress_counts_as_unlocked() {
        let u = parse_codex("[G5]\nCurProgress=5\nMaxProgress=5\n");
        let mut g5 = Unlock::new("G5", true, None);
        g5.progress = Some(Progress { current: 5, max: 5 });
        assert_eq!(u, vec![g5]);
    }

    #[test]
    fn skips_sections_without_achievement_keys() {
        assert!(parse_codex("[Achievements]\nER_ACHIEVEMENT_ROUNDTABLE=1\n").is_empty());
    }

    #[test]
    fn steam_section_names_are_matched_ignoring_case() {
        let u = parse_codex("[steamachievements]\nAchieved=1\n[STEAM64]\nAchieved=1\n");
        assert!(u.is_empty());
    }

    #[test]
    fn unparsable_progress_counts_as_absent() {
        let u = parse_codex("[X]\nAchieved=0\nCurProgress=abc\nMaxProgress=-1\n");
        assert_eq!(u, vec![Unlock::new("X", false, None)]);
    }

    #[test]
    fn missing_current_progress_counts_as_zero() {
        let u = parse_codex("[X]\nMaxProgress=4\n");
        let mut x = Unlock::new("X", false, None);
        x.progress = Some(Progress { current: 0, max: 4 });
        assert_eq!(u, vec![x]);
    }

    #[test]
    fn unlock_time_in_millis_is_normalized() {
        let u = parse_codex("[X]\nAchieved=true\nUnlockTime=1761321600000\n");
        assert_eq!(u, vec![Unlock::new("X", true, Some(1_761_321_600))]);
    }
}
