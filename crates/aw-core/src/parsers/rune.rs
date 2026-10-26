use super::vdf::{self, KvValue};
use super::{ParseError, truthy, unlock_time};
use crate::model::Unlock;

pub(crate) const MAX_BYTES: usize = 2 * 1024 * 1024;

pub(crate) fn parse_rune(text: &str) -> Result<Vec<Unlock>, ParseError> {
    let root = vdf::parse(text).map_err(|error| ParseError::Invalid(error.to_string()))?;
    let achievements = root
        .get("achievements")
        .filter(|value| matches!(value, KvValue::Obj(_)))
        .ok_or_else(|| ParseError::Invalid("missing achievements root".into()))?;
    Ok(achievements
        .entries()
        .iter()
        .filter(|(_, entry)| matches!(entry, KvValue::Obj(_)))
        .map(|(id, entry)| {
            let earned = entry
                .get("earned")
                .and_then(KvValue::as_str)
                .is_some_and(truthy);
            let time = entry
                .get("time")
                .and_then(KvValue::as_str)
                .and_then(unlock_time);
            Unlock::new(id.clone(), earned, time)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Unlock;
    use crate::parsers::{Format, ParseError, parse};

    #[test]
    fn reads_earned_and_time() {
        let u = parse_rune(
            "\"achievements\"\n{\n \"1\" { \"earned\" \"1\" \"time\" \"1700000000\" }\n \"2\" { \"earned\" \"0\" \"time\" \"0\" }\n}\n",
        )
        .unwrap();
        assert_eq!(
            u,
            vec![
                Unlock::new("1", true, Some(1_700_000_000)),
                Unlock::new("2", false, None)
            ]
        );
    }

    #[test]
    fn missing_root_is_error() {
        assert!(matches!(
            parse_rune("\"other\" { }"),
            Err(ParseError::Invalid(_))
        ));
    }

    #[test]
    fn broken_text_is_error() {
        assert!(matches!(
            parse_rune("\"achievements\" { \"1\" {"),
            Err(ParseError::Invalid(_))
        ));
    }

    #[test]
    fn over_two_megabytes_is_error() {
        let mut big = b"\"achievements\" { }".to_vec();
        big.resize(MAX_BYTES + 1, b' ');
        assert!(matches!(
            parse(Format::RuneCfg, &big),
            Err(ParseError::Invalid(_))
        ));
    }

    #[test]
    fn skips_non_object_entries_and_reads_yes() {
        let u =
            parse_rune("\"achievements\" { \"x\" \"1\" \"3\" { \"earned\" \"yes\" } }").unwrap();
        assert_eq!(u, vec![Unlock::new("3", true, None)]);
    }

    #[test]
    fn root_name_is_case_insensitive() {
        let u = parse_rune("\"Achievements\" { \"7\" { \"earned\" \"true\" \"time\" \"5\" } }")
            .unwrap();
        assert_eq!(u, vec![Unlock::new("7", true, Some(5))]);
    }
}
