use serde_json::{Map, Value};

use super::{LocalText, ParseError, epoch_time, truthy};
use crate::model::{Progress, Unlock};

const NAME_KEYS: [&str; 5] = ["name", "id", "apiname", "AchievementId", "achievement_id"];
const FLAG_KEYS: [&str; 4] = ["achieved", "earned", "unlocked", "HaveAchieved"];
const TIME_KEYS: [&str; 6] = [
    "unlock_time",
    "UnlockTime",
    "unlockTime",
    "earned_time",
    "timestamp",
    "time",
];
const CURRENT_KEYS: [&str; 2] = ["progress", "CurProgress"];
const MAX_KEYS: [&str; 2] = ["max_progress", "MaxProgress"];

type Entry = Map<String, Value>;

pub(crate) fn parse_generic_json(text: &str) -> Result<Vec<Unlock>, ParseError> {
    let root: Value =
        serde_json::from_str(text).map_err(|error| ParseError::Json(error.to_string()))?;
    Ok(entries(&root)
        .into_iter()
        .map(|(name, entry)| read_entry(name, entry))
        .collect())
}

pub(crate) fn texts(text: &str) -> Vec<LocalText> {
    let Ok(root) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    entries(&root)
        .into_iter()
        .filter_map(|(name, entry)| {
            Some(LocalText {
                api_name: name.to_owned(),
                display_name: string(entry, "display_name")?,
                description: string(entry, "description").unwrap_or_default(),
            })
        })
        .collect()
}

fn entries(root: &Value) -> Vec<(&str, &Entry)> {
    match root {
        Value::Object(map) => map
            .iter()
            .filter_map(|(name, entry)| Some((name.as_str(), entry.as_object()?)))
            .collect(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                let entry = item.as_object()?;
                let name = NAME_KEYS
                    .iter()
                    .find_map(|key| get(entry, key)?.as_str())
                    .filter(|name| !name.trim().is_empty())?;
                Some((name, entry))
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn read_entry(name: &str, entry: &Entry) -> Unlock {
    let flagged = FLAG_KEYS
        .iter()
        .find_map(|key| get(entry, key))
        .is_some_and(flag);
    let time = TIME_KEYS
        .iter()
        .find_map(|key| get(entry, key).and_then(integer).and_then(epoch_time));
    let progress = stat_progress(entry).or_else(|| {
        let current = CURRENT_KEYS
            .iter()
            .find_map(|key| get(entry, key).and_then(count))?;
        let max = MAX_KEYS
            .iter()
            .find_map(|key| get(entry, key).and_then(count))
            .filter(|max| *max > 0)?;
        Some(Progress { current, max })
    });
    let completed = progress.as_ref().is_some_and(|p| p.current >= p.max);
    let mut unlock = Unlock::new(name, flagged || time.is_some() || completed, time);
    unlock.progress = progress;
    unlock
}

fn stat_progress(entry: &Entry) -> Option<Progress> {
    get(entry, "stat_info")?
        .as_object()?
        .values()
        .filter_map(Value::as_object)
        .find_map(|stat| {
            let max = get(stat, "threshold_value")
                .and_then(count)
                .filter(|max| *max > 0)?;
            let current = get(stat, "current_value").and_then(count).unwrap_or(0);
            Some(Progress { current, max })
        })
}

fn get<'a>(entry: &'a Entry, key: &str) -> Option<&'a Value> {
    entry
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value)
}

fn flag(value: &Value) -> bool {
    match value {
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => truthy(text),
        _ => false,
    }
}

fn integer(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().filter(|n| n.is_finite()).map(|n| n as i64)),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn count(value: &Value) -> Option<u32> {
    value
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0 && *n <= f64::from(u32::MAX))
        .map(|n| n as u32)
}

fn string(entry: &Entry, key: &str) -> Option<String> {
    get(entry, key)?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Progress, Unlock};

    #[test]
    fn nemirtingas_locked_is_minus_one() {
        let u =
            parse_generic_json(r#"{"A":{"achievement_id":"A","unlock_time":-1,"progress":0.0}}"#)
                .unwrap();
        assert_eq!(u, vec![Unlock::new("A", false, None)]);
    }

    #[test]
    fn nemirtingas_unlocked_keeps_raw_time() {
        let u = parse_generic_json(
            r#"{"A":{"unlock_time":1700000000},"B":{"unlock_time":3600000000000}}"#,
        )
        .unwrap();
        assert_eq!(u[0], Unlock::new("A", true, Some(1_700_000_000)));
        assert!(u[1].unlocked);
        assert!(u[1].unlocked_at.is_some());
    }

    #[test]
    fn stat_info_progress() {
        let u = parse_generic_json(
            r#"{"A":{"unlock_time":-1,"stat_info":{"X":{"current_value":2,"name":"X","threshold_value":5}}}}"#,
        )
        .unwrap();
        assert_eq!(u[0].progress, Some(Progress { current: 2, max: 5 }));
        assert!(!u[0].unlocked);
    }

    #[test]
    fn plain_progress_and_completion() {
        let u = parse_generic_json(
            r#"{"A":{"progress":3,"max_progress":10},"B":{"CurProgress":10,"MaxProgress":10}}"#,
        )
        .unwrap();
        assert_eq!(
            u[0].progress,
            Some(Progress {
                current: 3,
                max: 10
            })
        );
        assert!(!u[0].unlocked);
        assert!(u[1].unlocked);
    }

    #[test]
    fn time_unlocks_even_with_false_flag() {
        let u = parse_generic_json(r#"{"A":{"achieved":false,"UnlockTime":1700000000}}"#).unwrap();
        assert_eq!(u, vec![Unlock::new("A", true, Some(1_700_000_000))]);
    }

    #[test]
    fn array_names() {
        let u = parse_generic_json(
            r#"[{"AchievementId":"A","UnlockTime":1700000000},{"id":"B","achieved":1},{"apiname":"C"},{"achievement_id":"D","earned":true},{"name":"E","unlocked":"yes"},{"earned":true}]"#,
        )
        .unwrap();
        assert_eq!(
            u,
            vec![
                Unlock::new("A", true, Some(1_700_000_000)),
                Unlock::new("B", true, None),
                Unlock::new("C", false, None),
                Unlock::new("D", true, None),
                Unlock::new("E", true, None)
            ]
        );
    }

    #[test]
    fn string_flags() {
        let u = parse_generic_json(
            r#"{"A":{"earned":"yes"},"B":{"unlocked":"0"},"C":{"HaveAchieved":1},"D":{"Achieved":"true"}}"#,
        )
        .unwrap();
        let flags: Vec<bool> = u.iter().map(|x| x.unlocked).collect();
        assert_eq!(flags, vec![true, false, true, true]);
    }

    #[test]
    fn time_keys_in_priority_order() {
        let u = parse_generic_json(
            r#"{"A":{"earned_time":1700000001},"B":{"timestamp":1700000002},"C":{"time":1700000003},"D":{"unlockTime":1700000004}}"#,
        )
        .unwrap();
        let times: Vec<Option<i64>> = u.iter().map(|x| x.unlocked_at).collect();
        assert_eq!(
            times,
            vec![
                Some(1_700_000_001),
                Some(1_700_000_002),
                Some(1_700_000_003),
                Some(1_700_000_004)
            ]
        );
    }

    #[test]
    fn invalid_json_is_a_json_error() {
        assert!(matches!(
            parse_generic_json("{nope"),
            Err(ParseError::Json(_))
        ));
    }

    #[test]
    fn texts_from_display_name() {
        let t = texts(
            r#"{"A":{"display_name":"Name","description":"Desc"},"B":{"display_name":""},"C":{"unlock_time":-1}}"#,
        );
        assert_eq!(
            t,
            vec![LocalText {
                api_name: "A".into(),
                display_name: "Name".into(),
                description: "Desc".into()
            }]
        );
    }

    #[test]
    fn texts_of_invalid_json_are_empty() {
        assert!(texts("nope").is_empty());
    }
}
