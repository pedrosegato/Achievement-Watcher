use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use crate::discovery::Found;
use crate::fileio::read_with_retry;
use crate::model::{Progress, Unlock, sane_unlock_time};
use crate::parsers::parse_file;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FileOutcome {
    Parsed { achievements: usize },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileResult {
    pub source_id: &'static str,
    pub path: PathBuf,
    pub outcome: FileOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameScan {
    pub app_id: u32,
    pub files: Vec<FileResult>,
    pub unlocks: Vec<Unlock>,
}

pub fn scan(found: &[Found], now: i64) -> Vec<GameScan> {
    let mut by_game: BTreeMap<u32, Vec<&Found>> = BTreeMap::new();
    for file in found {
        by_game.entry(file.app_id).or_default().push(file);
    }
    by_game
        .into_iter()
        .map(|(app_id, files)| scan_game(app_id, &files, now))
        .collect()
}

fn scan_game(app_id: u32, found: &[&Found], now: i64) -> GameScan {
    let mut merged = Merged::default();
    let mut files = Vec::with_capacity(found.len());
    for file in found {
        let outcome = match read_unlocks(file) {
            Ok(unlocks) => {
                let achievements = unlocks.len();
                for unlock in unlocks {
                    merged.add(unlock, now);
                }
                FileOutcome::Parsed { achievements }
            }
            Err(message) => FileOutcome::Error { message },
        };
        files.push(FileResult {
            source_id: file.source_id,
            path: file.path.clone(),
            outcome,
        });
    }
    GameScan {
        app_id,
        files,
        unlocks: merged.into_sorted(),
    }
}

fn read_unlocks(file: &Found) -> Result<Vec<Unlock>, String> {
    let bytes =
        read_with_retry(&file.path, Duration::from_secs(1)).map_err(|error| error.to_string())?;
    parse_file(file.format, &file.path, &bytes).map_err(|error| error.to_string())
}

#[derive(Default)]
struct Merged {
    unlocks: Vec<Unlock>,
    index: HashMap<String, usize>,
}

impl Merged {
    fn add(&mut self, mut incoming: Unlock, now: i64) {
        incoming.unlocked_at = incoming
            .unlocked_at
            .filter(|_| incoming.unlocked)
            .and_then(|raw| sane_unlock_time(raw, now));
        let key = incoming.api_name.to_ascii_uppercase();
        match self.index.get(&key) {
            Some(&position) => merge_into(&mut self.unlocks[position], incoming),
            None => {
                self.index.insert(key, self.unlocks.len());
                self.unlocks.push(incoming);
            }
        }
    }

    fn into_sorted(mut self) -> Vec<Unlock> {
        self.unlocks.sort_by_cached_key(|unlock| {
            (
                !unlock.unlocked,
                unlock.unlocked_at.unwrap_or(i64::MAX),
                unlock.api_name.to_ascii_uppercase(),
            )
        });
        self.unlocks
    }
}

fn merge_into(existing: &mut Unlock, incoming: Unlock) {
    existing.unlocked |= incoming.unlocked;
    existing.unlocked_at = [existing.unlocked_at, incoming.unlocked_at]
        .into_iter()
        .flatten()
        .min();
    existing.progress = match (existing.progress.take(), incoming.progress) {
        (Some(a), Some(b)) => Some(max_by_current(a, b)),
        (a, b) => a.or(b),
    };
}

fn max_by_current(a: Progress, b: Progress) -> Progress {
    if b.current > a.current { b } else { a }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::model::Progress;
    use crate::parsers::Format;

    const NOW: i64 = 1_800_000_000;

    fn file(
        dir: &Path,
        name: &str,
        body: &str,
        source_id: &'static str,
        app_id: u32,
        format: Format,
    ) -> Found {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        Found {
            source_id,
            app_id,
            external: None,
            path,
            format,
        }
    }

    #[test]
    fn merges_sources_keeping_earliest_time_and_any_unlock() {
        let tmp = tempfile::tempdir().unwrap();
        let a = file(
            tmp.path(),
            "a.ini",
            "[ACH1]\nachieved=true\ntimestamp=1761321600\n",
            "onlinefix",
            7,
            Format::OnlineFix,
        );
        let b = file(
            tmp.path(),
            "b.json",
            r#"{"ACH1":{"earned":true,"earned_time":1761300000},"ACH2":{"earned":false}}"#,
            "goldberg",
            7,
            Format::GoldbergJson,
        );
        let games = scan(&[a, b], NOW);
        assert_eq!(games.len(), 1);
        assert_eq!(
            games[0].unlocks,
            vec![
                Unlock::new("ACH1", true, Some(1_761_300_000)),
                Unlock::new("ACH2", false, None)
            ]
        );
    }

    #[test]
    fn achievement_names_merge_case_insensitively() {
        let tmp = tempfile::tempdir().unwrap();
        let a = file(
            tmp.path(),
            "a.ini",
            "[ach1]\nachieved=false\n",
            "onlinefix",
            7,
            Format::OnlineFix,
        );
        let b = file(
            tmp.path(),
            "b.json",
            r#"{"ACH1":{"earned":true,"earned_time":1761321600}}"#,
            "goldberg",
            7,
            Format::GoldbergJson,
        );
        let games = scan(&[a, b], NOW);
        assert_eq!(
            games[0].unlocks,
            vec![Unlock::new("ach1", true, Some(1_761_321_600))]
        );
    }

    #[test]
    fn scan_reports_empty_file_and_keeps_others() {
        let tmp = tempfile::tempdir().unwrap();
        let empty = file(tmp.path(), "e.ini", "", "onlinefix", 7, Format::OnlineFix);
        let good = file(
            tmp.path(),
            "g.json",
            r#"{"A":{"earned":true,"earned_time":1761321600}}"#,
            "goldberg",
            7,
            Format::GoldbergJson,
        );
        let games = scan(&[empty, good], NOW);
        let outcomes: Vec<&FileOutcome> = games[0].files.iter().map(|f| &f.outcome).collect();
        assert_eq!(
            outcomes,
            vec![
                &FileOutcome::Error {
                    message: "empty file".into()
                },
                &FileOutcome::Parsed { achievements: 1 }
            ]
        );
        assert_eq!(games[0].unlocks.len(), 1);
    }

    #[test]
    fn unreadable_path_is_reported_not_fatal() {
        let missing = Found {
            source_id: "gse",
            app_id: 9,
            external: None,
            path: PathBuf::from("does/not/exist.json"),
            format: Format::GoldbergJson,
        };
        let games = scan(&[missing], NOW);
        assert!(matches!(
            games[0].files[0].outcome,
            FileOutcome::Error { .. }
        ));
        assert!(games[0].unlocks.is_empty());
    }

    #[test]
    fn drops_insane_times() {
        let tmp = tempfile::tempdir().unwrap();
        let f = file(
            tmp.path(),
            "a.ini",
            &format!("[A]\nachieved=true\ntimestamp={}\n", NOW + 10_000),
            "onlinefix",
            7,
            Format::OnlineFix,
        );
        assert_eq!(
            scan(&[f], NOW)[0].unlocks,
            vec![Unlock::new("A", true, None)]
        );
    }

    #[test]
    fn sorts_unlocked_by_time_then_locked_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let f = file(
            tmp.path(),
            "a.json",
            r#"{"Z":{"earned":false},"B":{"earned":true,"earned_time":1761400000},"C":{"earned":true},"A":{"earned":true,"earned_time":1761300000},"Y":{"earned":false}}"#,
            "gse",
            7,
            Format::GoldbergJson,
        );
        let names: Vec<String> = scan(&[f], NOW)[0]
            .unlocks
            .iter()
            .map(|u| u.api_name.clone())
            .collect();
        assert_eq!(names, ["A", "B", "C", "Y", "Z"]);
    }

    #[test]
    fn locked_entry_never_keeps_a_time() {
        let tmp = tempfile::tempdir().unwrap();
        let f = file(
            tmp.path(),
            "a.ini",
            "[A]\nachieved=false\ntimestamp=1761321600\n",
            "onlinefix",
            7,
            Format::OnlineFix,
        );
        assert_eq!(
            scan(&[f], NOW)[0].unlocks,
            vec![Unlock::new("A", false, None)]
        );
    }

    #[test]
    fn groups_by_app_id_sorted_ascending() {
        let tmp = tempfile::tempdir().unwrap();
        let high = file(
            tmp.path(),
            "h.ini",
            "[A]\nachieved=true\n",
            "onlinefix",
            20,
            Format::OnlineFix,
        );
        let low = file(
            tmp.path(),
            "l.ini",
            "[A]\nachieved=true\n",
            "onlinefix",
            5,
            Format::OnlineFix,
        );
        let games = scan(&[high, low], NOW);
        let ids: Vec<u32> = games.iter().map(|g| g.app_id).collect();
        assert_eq!(ids, [5, 20]);
    }

    #[test]
    fn merge_keeps_progress_with_highest_current() {
        let tmp = tempfile::tempdir().unwrap();
        let a = file(
            tmp.path(),
            "a.ini",
            "[A]\nAchieved=0\nCurProgress=2\nMaxProgress=10\n",
            "codex",
            7,
            Format::CodexIni,
        );
        let b = file(
            tmp.path(),
            "b.ini",
            "[A]\nAchieved=0\nCurProgress=6\nMaxProgress=10\n",
            "rune",
            7,
            Format::CodexIni,
        );
        let c = file(
            tmp.path(),
            "c.ini",
            "[A]\nAchieved=0\nCurProgress=4\nMaxProgress=10\n",
            "codex",
            7,
            Format::CodexIni,
        );
        let games = scan(&[a, b, c], NOW);
        assert_eq!(
            games[0].unlocks[0].progress,
            Some(Progress {
                current: 6,
                max: 10
            })
        );
    }

    #[test]
    fn game_scan_serializes_outcome_status_tag() {
        let tmp = tempfile::tempdir().unwrap();
        let f = file(
            tmp.path(),
            "a.ini",
            "[A]\nachieved=true\n",
            "onlinefix",
            7,
            Format::OnlineFix,
        );
        let value = serde_json::to_value(scan(&[f], NOW)).unwrap();
        assert_eq!(value[0]["files"][0]["outcome"]["status"], "parsed");
        assert_eq!(value[0]["files"][0]["outcome"]["achievements"], 1);
    }
}
