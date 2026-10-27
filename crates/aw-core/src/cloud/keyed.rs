use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const NESTED_KEYS: &[&str] = &["gameSync"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stamped {
    pub value: Value,
    pub at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyedDoc {
    pub device_id: String,
    pub keys: BTreeMap<String, Stamped>,
}

pub fn flatten(settings: &Value, local_keys: &[&str]) -> BTreeMap<String, Value> {
    let mut flat = BTreeMap::new();
    let Some(object) = settings.as_object() else {
        return flat;
    };
    for (key, value) in object {
        if local_keys.contains(&key.as_str()) {
            continue;
        }
        if NESTED_KEYS.contains(&key.as_str()) {
            for (inner, item) in value.as_object().into_iter().flatten() {
                flat.insert(format!("{key}.{inner}"), item.clone());
            }
        } else {
            flat.insert(key.clone(), value.clone());
        }
    }
    flat
}

pub fn apply(settings: &mut Value, merged: &BTreeMap<String, Value>) {
    let Some(object) = settings.as_object_mut() else {
        return;
    };
    let mut nested: BTreeMap<&str, Map<String, Value>> =
        NESTED_KEYS.iter().map(|key| (*key, Map::new())).collect();
    for (key, value) in merged {
        match key.split_once('.') {
            Some((outer, inner)) if NESTED_KEYS.contains(&outer) => {
                if let Some(map) = nested.get_mut(outer) {
                    map.insert(inner.to_owned(), value.clone());
                }
            }
            _ => {
                object.insert(key.clone(), value.clone());
            }
        }
    }
    for (key, map) in nested {
        object.insert(key.to_owned(), Value::Object(map));
    }
}

pub fn stamp_changes(
    doc: &mut KeyedDoc,
    old: &BTreeMap<String, Value>,
    new: &BTreeMap<String, Value>,
    now: i64,
) {
    for (key, value) in new {
        if old.get(key) != Some(value) {
            doc.keys.insert(
                key.clone(),
                Stamped {
                    value: value.clone(),
                    at: now,
                },
            );
        }
    }
    for key in old.keys().filter(|key| !new.contains_key(*key)) {
        doc.keys.insert(
            key.clone(),
            Stamped {
                value: Value::Null,
                at: now,
            },
        );
    }
}

pub fn merge(docs: &[KeyedDoc]) -> BTreeMap<String, Value> {
    let mut best: BTreeMap<&str, (i64, &str, &Value)> = BTreeMap::new();
    for doc in docs {
        for (key, stamped) in &doc.keys {
            let candidate = (stamped.at, doc.device_id.as_str(), &stamped.value);
            let replace = best.get(key.as_str()).is_none_or(|(at, device, _)| {
                (stamped.at, doc.device_id.as_str()) > (*at, *device)
            });
            if replace {
                best.insert(key, candidate);
            }
        }
    }
    best.into_iter()
        .filter(|(_, (_, _, value))| !value.is_null())
        .map(|(key, (_, _, value))| (key.to_owned(), value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const LOCAL: &[&str] = &["extraGameDirs", "cloudFolder"];

    fn doc(device: &str, keys: &[(&str, Value, i64)]) -> KeyedDoc {
        KeyedDoc {
            device_id: device.into(),
            keys: keys
                .iter()
                .map(|(k, v, at)| {
                    (
                        (*k).to_owned(),
                        Stamped {
                            value: v.clone(),
                            at: *at,
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn local_keys_never_leave_the_pc() {
        let settings = json!({"preset": "x", "extraGameDirs": ["C:/g"], "cloudFolder": "D:/c"});
        let flat = flatten(&settings, LOCAL);
        assert_eq!(flat.keys().collect::<Vec<_>>(), vec!["preset"]);
    }

    #[test]
    fn game_sync_entries_are_separate_keys() {
        let settings = json!({"gameSync": {"374320": {"enabled": true}, "uplay-4": {"savePath": "<winAppData>/x"}}});
        let flat = flatten(&settings, LOCAL);
        assert_eq!(flat["gameSync.374320"], json!({"enabled": true}));
        assert_eq!(
            flat["gameSync.uplay-4"],
            json!({"savePath": "<winAppData>/x"})
        );
        assert!(!flat.contains_key("gameSync"));
    }

    #[test]
    fn newest_value_wins_per_key() {
        let desktop = doc(
            "a",
            &[("preset", json!("xbox"), 10), ("sound", json!(true), 1)],
        );
        let notebook = doc(
            "b",
            &[("sound", json!(false), 11), ("preset", json!("ps5"), 2)],
        );
        let merged = merge(&[desktop, notebook]);
        assert_eq!(merged["preset"], json!("xbox"));
        assert_eq!(merged["sound"], json!(false));
    }

    #[test]
    fn same_game_toggle_newest_wins() {
        let a = doc("a", &[("gameSync.7", json!({"enabled": true}), 5)]);
        let b = doc("b", &[("gameSync.7", json!({"enabled": false}), 6)]);
        assert_eq!(merge(&[a, b])["gameSync.7"], json!({"enabled": false}));
    }

    #[test]
    fn tie_breaks_by_device_id() {
        let a = doc("a", &[("preset", json!("one"), 5)]);
        let b = doc("b", &[("preset", json!("two"), 5)]);
        assert_eq!(merge(&[b.clone(), a.clone()])["preset"], json!("two"));
        assert_eq!(merge(&[a, b])["preset"], json!("two"));
    }

    #[test]
    fn null_removes_a_key() {
        let a = doc("a", &[("gameSync.7", json!({"enabled": true}), 5)]);
        let b = doc("b", &[("gameSync.7", Value::Null, 6)]);
        assert!(!merge(&[a, b]).contains_key("gameSync.7"));
    }

    #[test]
    fn stamp_changes_marks_only_changed_keys() {
        let old = flatten(
            &json!({"preset": "x", "sound": true, "gameSync": {"7": {"enabled": true}}}),
            LOCAL,
        );
        let new = flatten(
            &json!({"preset": "y", "sound": true, "gameSync": {}}),
            LOCAL,
        );
        let mut own = doc("a", &[("sound", json!(true), 1)]);
        stamp_changes(&mut own, &old, &new, 50);
        assert_eq!(
            own.keys["preset"],
            Stamped {
                value: json!("y"),
                at: 50
            }
        );
        assert_eq!(own.keys["sound"].at, 1);
        assert_eq!(
            own.keys["gameSync.7"],
            Stamped {
                value: Value::Null,
                at: 50
            }
        );
    }

    #[test]
    fn apply_round_trips_flatten() {
        let original =
            json!({"preset": "x", "extraGameDirs": ["C:/g"], "gameSync": {"7": {"enabled": true}}});
        let mut target = json!({"preset": "old", "extraGameDirs": ["D:/mine"], "gameSync": {"9": {"enabled": false}}});
        apply(&mut target, &flatten(&original, LOCAL));
        assert_eq!(target["preset"], json!("x"));
        assert_eq!(target["extraGameDirs"], json!(["D:/mine"]));
        assert_eq!(target["gameSync"], json!({"7": {"enabled": true}}));
    }
}
