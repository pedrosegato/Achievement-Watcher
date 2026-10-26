use super::unlock_time;
use crate::model::{Progress, Unlock};
use crate::parsers::ini::{Ini, Section};

const SKIPPED_SECTIONS: [&str; 5] = [
    "Steam",
    "Steam64",
    "SteamAchievements",
    "Settings",
    "GameSettings",
];
const UNLOCK_KEYS: [&str; 5] = ["Achieved", "Unlocked", "HaveAchieved", "earned", "State"];
const TIME_KEYS: [&str; 6] = [
    "UnlockTime",
    "unlock_time",
    "HaveAchievedTime",
    "HaveHaveAchievedTime",
    "earned_time",
    "Time",
];

pub(crate) fn parse_generic_ini(text: &str) -> Vec<Unlock> {
    Ini::parse(text)
        .sections()
        .iter()
        .filter(|s| !s.name.is_empty())
        .filter(|s| {
            !SKIPPED_SECTIONS
                .iter()
                .any(|n| s.name.eq_ignore_ascii_case(n))
        })
        .filter_map(read_section)
        .collect()
}

fn first<'a>(section: &'a Section, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| section.get(key))
}

fn read_section(section: &Section) -> Option<Unlock> {
    let flag = first(section, &UNLOCK_KEYS);
    let time = first(section, &TIME_KEYS).and_then(unlock_time);
    let current =
        first(section, &["CurProgress", "progress"]).and_then(|v| v.trim().parse::<u32>().ok());
    let max =
        first(section, &["MaxProgress", "max_progress"]).and_then(|v| v.trim().parse::<u32>().ok());
    if flag.is_none() && time.is_none() && max.is_none() {
        return None;
    }
    let flagged = flag.is_some_and(|v| {
        let v = v.trim();
        v == "1" || v.eq_ignore_ascii_case("true")
    });
    let completed = matches!((current, max), (Some(c), Some(m)) if m > 0 && c >= m);
    let mut unlock = Unlock::new(
        section.name.clone(),
        flagged || time.is_some() || completed,
        time,
    );
    unlock.progress = match (current, max) {
        (Some(current), Some(max)) if max > 0 => Some(Progress { current, max }),
        _ => None,
    };
    Some(unlock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ali213_have_achieved_keys() {
        let u = parse_generic_ini(
            "[Settings]\nAppID=1\n[A1]\nHaveAchieved=1\nHaveAchievedTime=1700000000\n[A2]\nHaveAchieved=0\n",
        );
        assert_eq!(
            u,
            vec![
                Unlock::new("A1", true, Some(1_700_000_000)),
                Unlock::new("A2", false, None)
            ]
        );
    }

    #[test]
    fn accepts_typo_key_and_true_values() {
        let u = parse_generic_ini("[A]\nHaveHaveAchievedTime=5\n[B]\nUnlocked=TRUE\n");
        assert_eq!(
            u,
            vec![
                Unlock::new("A", true, Some(5)),
                Unlock::new("B", true, None)
            ]
        );
    }

    #[test]
    fn full_progress_counts_as_unlocked() {
        let u = parse_generic_ini(
            "[A]\nAchieved=0\nCurProgress=10\nMaxProgress=10\n[B]\nAchieved=0\nCurProgress=3\nMaxProgress=10\n",
        );
        assert!(u[0].unlocked);
        assert!(!u[1].unlocked);
        assert_eq!(
            u[1].progress,
            Some(Progress {
                current: 3,
                max: 10
            })
        );
    }

    #[test]
    fn unrelated_sections_are_ignored() {
        assert!(parse_generic_ini("[Other]\nfoo=bar\n").is_empty());
    }
}
