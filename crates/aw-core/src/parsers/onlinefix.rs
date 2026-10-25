use super::unlock_time;
use crate::model::Unlock;
use crate::parsers::ini::Ini;

pub(crate) fn parse_onlinefix(text: &str) -> Vec<Unlock> {
    Ini::parse(text)
        .sections()
        .iter()
        .filter(|section| !section.name.is_empty())
        .filter_map(|section| {
            let achieved = section.get("achieved")?;
            let unlocked = achieved.eq_ignore_ascii_case("true") || achieved == "1";
            let raw_time = section
                .get("timestamp")
                .or_else(|| section.get("TimeUnlocked"));
            Some(Unlock::new(
                section.name.clone(),
                unlocked,
                raw_time.and_then(unlock_time),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Unlock;

    #[test]
    fn reads_lowercase_variant() {
        let u = parse_onlinefix("[ACH39]\nachieved=true\ntimestamp=1761321600\n");
        assert_eq!(u, vec![Unlock::new("ACH39", true, Some(1_761_321_600))]);
    }

    #[test]
    fn reads_capitalized_variant_with_millis() {
        let u = parse_onlinefix("[ACH01]\nAchieved=true\nTimeUnlocked=1761321600000\n");
        assert_eq!(u, vec![Unlock::new("ACH01", true, Some(1_761_321_600))]);
    }

    #[test]
    fn skips_sections_without_achieved_key() {
        assert!(parse_onlinefix("[Achievements]\nROUNDTABLE_HOLD=1\n").is_empty());
    }

    #[test]
    fn locked_with_zero_time_has_no_time() {
        let u = parse_onlinefix("[ACH25]\nachieved=false\ntimestamp=0\n");
        assert_eq!(u, vec![Unlock::new("ACH25", false, None)]);
    }
}
