use std::path::{Path, PathBuf};

use super::binkv::parse_binary_kv;
use super::vdf::KvValue;
use super::{ParseError, epoch_time};
use crate::model::Unlock;

pub fn schema_path(user_stats: &Path) -> Option<PathBuf> {
    let name = user_stats.file_name()?.to_str()?;
    let rest = name.strip_prefix("UserGameStats_")?.strip_suffix(".bin")?;
    let (_, app_id) = rest.rsplit_once('_')?;
    Some(user_stats.with_file_name(format!("UserGameStatsSchema_{app_id}.bin")))
}

pub fn parse_steam_stats(user: &[u8], schema: &[u8]) -> Result<Vec<Unlock>, ParseError> {
    let schema = parse_binary_kv(schema)?;
    let user = parse_binary_kv(user)?;
    let root = schema
        .entries()
        .first()
        .map(|(_, v)| v)
        .ok_or(ParseError::Empty)?;
    let stats = root
        .get("stats")
        .ok_or_else(|| ParseError::Invalid("schema has no stats".into()))?;
    let cache = user.get("cache").unwrap_or(&user);
    let mut unlocks = Vec::new();
    for (stat_id, stat) in stats.entries() {
        let Some(bits) = stat.get("bits") else {
            continue;
        };
        let state = cache.get(stat_id);
        let data = state
            .and_then(|s| s.get("data"))
            .and_then(KvValue::as_str)
            .and_then(|v| v.parse::<i64>().ok())
            .map_or(0u32, |v| v as u32);
        let times = state.and_then(|s| s.get("AchievementTimes"));
        for (bit_id, bit) in bits.entries() {
            let Some(name) = bit.get("name").and_then(KvValue::as_str) else {
                continue;
            };
            let Ok(index) = bit_id.parse::<u32>() else {
                continue;
            };
            let unlocked = index < 32 && (data >> index) & 1 == 1;
            let time = times
                .and_then(|t| t.get(bit_id))
                .and_then(KvValue::as_str)
                .and_then(|v| v.parse::<i64>().ok())
                .map(|v| i64::from(v as u32))
                .and_then(epoch_time)
                .filter(|_| unlocked);
            unlocks.push(Unlock::new(name, unlocked, time));
        }
    }
    Ok(unlocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> Vec<u8> {
        let mut v = text.as_bytes().to_vec();
        v.push(0);
        v
    }

    fn int(key: &str, value: i32) -> Vec<u8> {
        let mut v = vec![0x02];
        v.extend(s(key));
        v.extend(value.to_le_bytes());
        v
    }

    fn str_field(key: &str, value: &str) -> Vec<u8> {
        let mut v = vec![0x01];
        v.extend(s(key));
        v.extend(s(value));
        v
    }

    fn open(key: &str) -> Vec<u8> {
        let mut v = vec![0x00];
        v.extend(s(key));
        v
    }

    fn schema() -> Vec<u8> {
        let mut b = open("10");
        b.extend(open("stats"));
        for (stat, names) in [("1", ["A", "B"]), ("2", ["C", "D"])] {
            b.extend(open(stat));
            b.extend(str_field("type", "ACHIEVEMENTS"));
            b.extend(open("bits"));
            for (i, name) in names.iter().enumerate() {
                b.extend(open(&i.to_string()));
                b.extend(str_field("name", name));
                b.push(0x08);
            }
            b.extend([0x08, 0x08]);
        }
        b.extend(open("7"));
        b.extend(str_field("name", "kills"));
        b.push(0x08);
        b.extend([0x08, 0x08, 0x08]);
        b
    }

    fn user() -> Vec<u8> {
        let mut b = open("cache");
        b.extend(int("crc", 1));
        b.extend(open("1"));
        b.extend(int("data", 0b10));
        b.extend(open("AchievementTimes"));
        b.extend(int("1", 1_700_000_000));
        b.extend([0x08, 0x08]);
        b.extend(open("2"));
        b.extend(int("data", 0b01));
        b.extend([0x08, 0x08, 0x08]);
        b
    }

    #[test]
    fn maps_bits_of_several_stats() {
        let u = parse_steam_stats(&user(), &schema()).unwrap();
        assert_eq!(
            u,
            vec![
                Unlock::new("A", false, None),
                Unlock::new("B", true, Some(1_700_000_000)),
                Unlock::new("C", true, None),
                Unlock::new("D", false, None),
            ]
        );
    }

    #[test]
    fn schema_path_from_user_stats_name() {
        assert_eq!(
            schema_path(Path::new(
                r"C:\Steam\appcache\stats\UserGameStats_1087865111_1091500.bin"
            )),
            Some(PathBuf::from(
                r"C:\Steam\appcache\stats\UserGameStatsSchema_1091500.bin"
            ))
        );
        assert_eq!(schema_path(Path::new("other.bin")), None);
    }

    #[test]
    fn real_steam_files_parse() {
        let dir = Path::new(r"C:\Program Files (x86)\Steam\appcache\stats");
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut checked = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(schema) = schema_path(&path) else {
                continue;
            };
            let (Ok(user), Ok(schema)) = (std::fs::read(&path), std::fs::read(&schema)) else {
                continue;
            };
            parse_steam_stats(&user, &schema).unwrap();
            checked += 1;
        }
        assert!(checked > 0 || !dir.exists());
    }
}
