use super::unlock_time;
use crate::model::Unlock;
use crate::parsers::ini::Ini;

pub(crate) fn parse_achiev(text: &str) -> Vec<Unlock> {
    let ini = Ini::parse(text);
    let Some(achievements) = ini.section("Achievements") else {
        return Vec::new();
    };
    let times = ini
        .section("AchievementsUnlockTimes")
        .map(|section| section.lookup())
        .unwrap_or_default();
    achievements
        .entries
        .iter()
        .map(|(name, value)| {
            let (unlocked, token_time) = if value.contains('@') {
                read_at_separated(value)
            } else {
                (value == "1" || value.eq_ignore_ascii_case("true"), None)
            };
            let section_time = times
                .get(&name.to_ascii_lowercase())
                .and_then(|raw| unlock_time(raw));
            Unlock::new(name.clone(), unlocked, section_time.or(token_time))
        })
        .collect()
}

fn read_at_separated(value: &str) -> (bool, Option<i64>) {
    let mut tokens = value.split('@');
    let unlocked = tokens.next().is_some_and(|token| token.trim() == "1");
    (unlocked, tokens.next_back().and_then(unlock_time))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Unlock;

    #[test]
    fn lookup_is_case_insensitive_across_sections() {
        assert_eq!(
            parse_achiev(
                "[Achievements]
Abc=1
[AchievementsUnlockTimes]
ABC=1700000000
"
            ),
            vec![Unlock::new("Abc", true, Some(1_700_000_000))]
        );
    }

    #[test]
    fn reads_skidrow_at_separated_values() {
        let u = parse_achiev("[Achievements]\nA1=1@0@1700000000\nA2=0@0@0\n");
        assert_eq!(
            u,
            vec![
                Unlock::new("A1", true, Some(1_700_000_000)),
                Unlock::new("A2", false, None)
            ]
        );
    }

    #[test]
    fn reads_hoodlum_layout_with_unlock_times_section() {
        let u =
            parse_achiev("[Achievements]\nA1=1\nA2=0\n[AchievementsUnlockTimes]\nA1=1700000000\n");
        assert_eq!(
            u,
            vec![
                Unlock::new("A1", true, Some(1_700_000_000)),
                Unlock::new("A2", false, None)
            ]
        );
    }

    #[test]
    fn hoodlum_without_times_section_has_no_time() {
        assert_eq!(
            parse_achiev("[Achievements]\nA1=1\n"),
            vec![Unlock::new("A1", true, None)]
        );
    }

    #[test]
    fn empty_last_token_has_no_time() {
        assert_eq!(
            parse_achiev("[Achievements]\nA1=1@\n"),
            vec![Unlock::new("A1", true, None)]
        );
    }

    #[test]
    fn times_section_wins_over_at_token() {
        let u = parse_achiev(
            "[Achievements]
A1=1@0@0
A2=1@0@1600000000
[AchievementsUnlockTimes]
A1=1700000000
A2=1700000000
",
        );
        assert_eq!(
            u,
            vec![
                Unlock::new("A1", true, Some(1_700_000_000)),
                Unlock::new("A2", true, Some(1_700_000_000))
            ]
        );
    }

    #[test]
    fn times_entry_missing_or_invalid_falls_back_to_at_token() {
        let u = parse_achiev(
            "[Achievements]
A1=1@0@1700000000
A2=1@0@1700000000
[AchievementsUnlockTimes]
A2=abc
",
        );
        assert_eq!(
            u,
            vec![
                Unlock::new("A1", true, Some(1_700_000_000)),
                Unlock::new("A2", true, Some(1_700_000_000))
            ]
        );
    }

    #[test]
    fn two_token_value_reads_the_last_token_as_time() {
        assert_eq!(
            parse_achiev("[Achievements]\nA1=1@1700000000\n"),
            vec![Unlock::new("A1", true, Some(1_700_000_000))]
        );
    }

    #[test]
    fn missing_achievements_section_is_empty() {
        assert!(parse_achiev("[Other]\nA1=1\n").is_empty());
    }
}
