use super::{epoch_time, unlock_time};
use crate::model::Unlock;
use crate::parsers::ini::Ini;

pub(crate) fn parse_creamapi(text: &str) -> Vec<Unlock> {
    Ini::parse(text)
        .sections()
        .iter()
        .filter(|section| !section.name.is_empty())
        .filter_map(|section| {
            let achieved = section.get("achieved")?;
            let unlocked = achieved.eq_ignore_ascii_case("true") || achieved == "1";
            Some(Unlock::new(
                section.name.clone(),
                unlocked,
                section.get("unlocktime").and_then(creamapi_time),
            ))
        })
        .collect()
}

fn creamapi_time(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    if raw.len() == 7 && raw.bytes().all(|b| b.is_ascii_digit()) {
        raw.parse::<i64>().ok().and_then(|n| epoch_time(n * 1000))
    } else {
        unlock_time(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Unlock;

    #[test]
    fn reads_achieved_and_plain_seconds() {
        let u = parse_creamapi("[A]\nachieved=true\nunlocktime=1700000000\n");
        assert_eq!(u, vec![Unlock::new("A", true, Some(1_700_000_000))]);
    }

    #[test]
    fn seven_digit_unlocktime_is_multiplied_by_1000() {
        let u = parse_creamapi("[A]\nachieved=true\nunlocktime=1681307\n");
        assert_eq!(u, vec![Unlock::new("A", true, Some(1_681_307_000))]);
    }

    #[test]
    fn locked_and_sections_without_achieved_key() {
        let u = parse_creamapi("[A]\nachieved=false\nunlocktime=0\n[Meta]\nfoo=1\n");
        assert_eq!(u, vec![Unlock::new("A", false, None)]);
    }

    #[test]
    fn achieved_is_case_insensitive_and_accepts_one() {
        let u = parse_creamapi("[A]\nACHIEVED=TRUE\n[B]\nachieved=1\n");
        assert_eq!(
            u,
            vec![Unlock::new("A", true, None), Unlock::new("B", true, None)]
        );
    }
}
