use std::path::Path;

use aw_core::discovery::Found;
use aw_core::discovery::lumaplay::{self, Registry};
use aw_core::model::Unlock;
use aw_core::parsers::parse_file;
use aw_core::store::Store;
use serde::Serialize;

pub const MAX_COPY: usize = 2 * 1024 * 1024;
const REPORT_FILE: &str = "report.json";
const REGISTRY_SUFFIX: &str = ".txt";

#[derive(Serialize)]
struct Report<'a> {
    version: &'a str,
    generated_at: i64,
    game: GameInfo,
    files: Vec<FileInfo>,
    recorded: Vec<Recorded>,
}

#[derive(Serialize)]
struct GameInfo {
    app_id: u32,
    name: Option<String>,
    platform: Option<&'static str>,
    external_id: Option<String>,
}

#[derive(Serialize)]
struct FileInfo {
    path: String,
    source_id: &'static str,
    format: String,
    size: Option<usize>,
    copied_as: Option<String>,
    last_ok_at: Option<i64>,
    last_error: Option<String>,
    last_count: Option<u32>,
    current: Current,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Current {
    Parsed { unlocks: Vec<Unlock> },
    Error { message: String },
}

#[derive(Serialize)]
struct Recorded {
    api_name: String,
    unlocked_at: Option<i64>,
}

pub fn build(
    store: &Store,
    found: &[Found],
    registry: &dyn Registry,
    app_id: u32,
    version: &str,
    now: i64,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let error = |e: aw_core::store::StoreError| e.to_string();
    let history = store.files_report().map_err(error)?;
    let external = store.external_game(app_id).map_err(error)?;
    let mut copies = Vec::new();
    let mut files = Vec::new();
    for file in found.iter().filter(|file| file.app_id == app_id) {
        let path = file.path.to_string_lossy().to_string();
        let registry_key = lumaplay::is_registry_path(&file.path);
        let bytes = if registry_key {
            lumaplay::snapshot(registry, &file.path)
        } else {
            std::fs::read(&file.path)
        };
        let past = history.iter().find(|row| row.path == path);
        let (size, copied_as, current) = match bytes {
            Ok(bytes) => {
                let current = match parse_file(file.format, &file.path, &bytes) {
                    Ok(unlocks) => Current::Parsed { unlocks },
                    Err(error) => Current::Error {
                        message: error.to_string(),
                    },
                };
                let copied_as = (bytes.len() <= MAX_COPY).then(|| {
                    let name = copy_name(copies.len() + 1, &file.path, registry_key);
                    copies.push((name.clone(), bytes.clone()));
                    name
                });
                (Some(bytes.len()), copied_as, current)
            }
            Err(error) => (
                None,
                None,
                Current::Error {
                    message: error.to_string(),
                },
            ),
        };
        files.push(FileInfo {
            path,
            source_id: file.source_id,
            format: format!("{:?}", file.format),
            size,
            copied_as,
            last_ok_at: past.and_then(|row| row.last_ok_at),
            last_error: past.and_then(|row| row.last_error.clone()),
            last_count: past.and_then(|row| row.last_count),
            current,
        });
    }
    let recorded = store
        .game_achievements(app_id)
        .map_err(error)?
        .into_iter()
        .filter(|row| row.unlocked_at.is_some())
        .map(|row| Recorded {
            api_name: row.api_name,
            unlocked_at: row.unlocked_at,
        })
        .collect();
    let report = Report {
        version,
        generated_at: now,
        game: GameInfo {
            app_id,
            name: store.game_name(app_id).map_err(error)?,
            platform: external.as_ref().map(|game| game.platform.as_str()),
            external_id: external.map(|game| game.external_id),
        },
        files,
        recorded,
    };
    let mut entries = vec![(
        REPORT_FILE.to_owned(),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )];
    entries.extend(copies);
    Ok(entries)
}

fn copy_name(index: usize, path: &Path, registry_key: bool) -> String {
    let name = path.file_name().map_or_else(
        || "file".to_owned(),
        |name| name.to_string_lossy().to_string(),
    );
    let suffix = if registry_key { REGISTRY_SUFFIX } else { "" };
    format!("files/{index}-{name}{suffix}")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use aw_core::discovery::Found;
    use aw_core::discovery::lumaplay::MemoryRegistry;
    use aw_core::parsers::Format;
    use aw_core::platform::{ExternalId, GameImages, Platform};
    use aw_core::store::Store;
    use serde_json::Value;

    use super::*;

    fn write(dir: &Path, name: &str, body: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
        path
    }

    fn names(entries: &[(String, Vec<u8>)]) -> Vec<&str> {
        entries.iter().map(|(name, _)| name.as_str()).collect()
    }

    fn json(entries: &[(String, Vec<u8>)]) -> Value {
        let (_, bytes) = entries
            .iter()
            .find(|(name, _)| name == "report.json")
            .unwrap();
        serde_json::from_slice(bytes).unwrap()
    }

    #[test]
    fn report_contains_json_and_only_this_games_files() {
        let tmp = tempfile::tempdir().unwrap();
        let ini = write(
            tmp.path(),
            "a/Achievements.ini",
            b"[A]\nachieved=true\ntimestamp=1700000000\n",
        );
        let json_file = write(
            tmp.path(),
            "b/achievements.json",
            br#"{"B":{"earned":false}}"#,
        );
        let other = write(tmp.path(), "c/Achievements.ini", b"[Z]\nachieved=true\n");
        let found = vec![
            Found::steam("onlinefix", 10, ini.clone(), Format::OnlineFix),
            Found::steam("gse", 10, json_file, Format::GoldbergJson),
            Found::steam("onlinefix", 20, other, Format::OnlineFix),
        ];
        let store = Store::in_memory().unwrap();
        store.ensure_game(10).unwrap();
        store
            .record_unlock(10, "A", 1_700_000_000, 1_700_000_001, "onlinefix")
            .unwrap();
        store
            .mark_file_ok(&ini.to_string_lossy(), "onlinefix", 10, "h", 1, 5)
            .unwrap();
        let entries = build(&store, &found, &MemoryRegistry::default(), 10, "3.1.0", 9).unwrap();
        assert_eq!(
            names(&entries),
            vec![
                "report.json",
                "files/1-Achievements.ini",
                "files/2-achievements.json"
            ]
        );
        let report = json(&entries);
        assert_eq!(report["version"], "3.1.0");
        assert_eq!(report["game"]["app_id"], 10);
        assert_eq!(report["files"].as_array().unwrap().len(), 2);
        assert_eq!(report["files"][0]["last_ok_at"], 5);
        assert_eq!(report["files"][0]["current"]["status"], "parsed");
        assert_eq!(report["files"][0]["current"]["unlocks"][0]["api_name"], "A");
        assert_eq!(report["files"][1]["copied_as"], "files/2-achievements.json");
        assert_eq!(report["recorded"][0]["api_name"], "A");
    }

    #[test]
    fn report_caps_copied_file_size() {
        let tmp = tempfile::tempdir().unwrap();
        let mut body = b"[A]\nachieved=true\n".to_vec();
        body.resize(MAX_COPY + 1, b' ');
        let big = write(tmp.path(), "Achievements.ini", &body);
        let found = vec![Found::steam("onlinefix", 10, big, Format::OnlineFix)];
        let store = Store::in_memory().unwrap();
        let entries = build(&store, &found, &MemoryRegistry::default(), 10, "v", 1).unwrap();
        assert_eq!(names(&entries), vec!["report.json"]);
        let report = json(&entries);
        assert!(report["files"][0]["copied_as"].is_null());
        assert_eq!(report["files"][0]["size"], MAX_COPY + 1);
        assert_eq!(report["files"][0]["current"]["status"], "parsed");
    }

    #[test]
    fn report_reads_lumaplay_snapshot_and_external_identity() {
        let mut registry = MemoryRegistry::default();
        registry.values.insert(
            r"SOFTWARE\LumaPlay\p\4\Achievements".into(),
            vec![("ACH_1".into(), 1)],
        );
        let key = ExternalId {
            platform: Platform::Ubisoft,
            id: "4".into(),
        };
        let store = Store::in_memory().unwrap();
        let app_id = store
            .save_external_game(&key, None, Some("Lost"), &GameImages::default(), 1)
            .unwrap();
        let mut file = Found::external(
            "lumaplay",
            key,
            std::path::PathBuf::from(r"HKCU\SOFTWARE\LumaPlay\p\4\Achievements"),
            Format::LumaPlay,
        );
        file.app_id = app_id;
        let entries = build(&store, &[file], &registry, app_id, "v", 1).unwrap();
        assert_eq!(
            names(&entries),
            vec!["report.json", "files/1-Achievements.txt"]
        );
        assert_eq!(entries[1].1, b"ACH_1=1\n");
        let report = json(&entries);
        assert_eq!(report["game"]["name"], "Lost");
        assert_eq!(report["game"]["platform"], "uplay");
        assert_eq!(report["game"]["external_id"], "4");
    }

    #[test]
    fn unreadable_file_is_reported_as_error() {
        let found = vec![Found::steam(
            "onlinefix",
            10,
            std::path::PathBuf::from("Z:/missing/Achievements.ini"),
            Format::OnlineFix,
        )];
        let store = Store::in_memory().unwrap();
        let entries = build(&store, &found, &MemoryRegistry::default(), 10, "v", 1).unwrap();
        let report = json(&entries);
        assert_eq!(report["files"][0]["current"]["status"], "error");
        assert!(report["files"][0]["copied_as"].is_null());
    }
}
