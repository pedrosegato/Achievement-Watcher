use serde_json::Value;

use super::{ParseError, epoch_time};
use crate::model::Unlock;

pub(crate) fn parse_goldberg(text: &str) -> Result<Vec<Unlock>, ParseError> {
    let root: Value =
        serde_json::from_str(text).map_err(|error| ParseError::Json(error.to_string()))?;
    Ok(match root {
        Value::Object(entries) => entries
            .iter()
            .filter_map(|(name, entry)| read_entry(name, entry))
            .collect(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| read_entry(item.get("name")?.as_str()?, item))
            .collect(),
        _ => Vec::new(),
    })
}

fn read_entry(name: &str, entry: &Value) -> Option<Unlock> {
    let entry = entry.as_object()?;
    let earned = match entry.get("earned") {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|value| value != 0.0),
        _ => false,
    };
    let time = entry
        .get("earned_time")
        .and_then(Value::as_i64)
        .and_then(epoch_time);
    Some(Unlock::new(name, earned, time))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Unlock;
    use crate::parsers::{Format, ParseError, parse};

    #[test]
    fn reads_object_form_in_key_order() {
        let u = parse_goldberg(
            r#"{"ACH_B":{"earned":false,"earned_time":0},"ACH_A":{"earned":true,"earned_time":1761321600}}"#,
        )
        .unwrap();
        assert_eq!(
            u,
            vec![
                Unlock::new("ACH_A", true, Some(1_761_321_600)),
                Unlock::new("ACH_B", false, None)
            ]
        );
    }

    #[test]
    fn reads_array_form() {
        let u = parse_goldberg(
            r#"[{"name":"ROUNDTABLE_HOLD","earned":true,"earned_time":1700000000}]"#,
        )
        .unwrap();
        assert_eq!(
            u,
            vec![Unlock::new("ROUNDTABLE_HOLD", true, Some(1_700_000_000))]
        );
    }

    #[test]
    fn accepts_numeric_earned_and_missing_time() {
        let u = parse_goldberg(r#"{"X":{"earned":1}}"#).unwrap();
        assert_eq!(u, vec![Unlock::new("X", true, None)]);
    }

    #[test]
    fn invalid_json_is_a_json_error() {
        assert!(matches!(
            parse(Format::GoldbergJson, b"{not json"),
            Err(ParseError::Json(_))
        ));
    }

    #[test]
    fn missing_or_zero_earned_means_locked() {
        let u = parse_goldberg(r#"{"A":{"earned_time":1700000000},"B":{"earned":0}}"#).unwrap();
        assert_eq!(
            u,
            vec![
                Unlock::new("A", false, Some(1_700_000_000)),
                Unlock::new("B", false, None)
            ]
        );
    }

    #[test]
    fn normalizes_millisecond_time() {
        let u = parse_goldberg(r#"{"A":{"earned":true,"earned_time":1761321600000}}"#).unwrap();
        assert_eq!(u, vec![Unlock::new("A", true, Some(1_761_321_600))]);
    }

    #[test]
    fn skips_entries_that_are_not_objects_or_lack_a_name() {
        let object_form = parse_goldberg(r#"{"A":5,"B":{"earned":true}}"#).unwrap();
        assert_eq!(object_form, vec![Unlock::new("B", true, None)]);

        let array_form =
            parse_goldberg(r#"[3,{"earned":true},{"name":7,"earned":true},{"name":"C"}]"#).unwrap();
        assert_eq!(array_form, vec![Unlock::new("C", false, None)]);
    }

    #[test]
    fn valid_json_of_another_shape_has_no_unlocks() {
        assert!(parse_goldberg("5").unwrap().is_empty());
        assert!(parse_goldberg("{}").unwrap().is_empty());
        assert!(parse_goldberg("[]").unwrap().is_empty());
    }
}
