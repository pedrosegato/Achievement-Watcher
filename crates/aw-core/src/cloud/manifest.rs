use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cloud::layout::{read_json, write_json_atomic};
use crate::cloud::{CloudError, CloudResult};

pub const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";
const INDEX_FILE: &str = "ludusavi-index.json";
const ETAG_FILE: &str = "ludusavi.etag";
const MAX_MANIFEST_BYTES: u64 = 128 * 1024 * 1024;
const SAVE_TAG: &str = "save";
const WINDOWS: &str = "windows";
const STEAM: &str = "steam";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ManifestIndex {
    pub games: BTreeMap<u32, ManifestGame>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestGame {
    pub name: String,
    pub install_dir: Option<String>,
    pub files: Vec<ManifestFile>,
    pub has_registry: bool,
    pub steam_cloud: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub pattern: String,
    pub steam_store_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    Updated,
    NotModified,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RawEntry {
    files: Option<BTreeMap<String, Option<RawFile>>>,
    registry: Option<serde_yaml_ng::Value>,
    steam: Option<RawSteam>,
    id: Option<RawIds>,
    alias: Option<String>,
    cloud: Option<RawCloud>,
    install_dir: Option<BTreeMap<String, serde_yaml_ng::Value>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawFile {
    tags: Vec<String>,
    when: Option<Vec<RawWhen>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawWhen {
    os: Option<String>,
    store: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawSteam {
    id: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RawIds {
    steam_extra: Vec<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawCloud {
    steam: bool,
}

pub fn parse_manifest(yaml: &str) -> CloudResult<ManifestIndex> {
    let raw: BTreeMap<String, Option<RawEntry>> = serde_yaml_ng::from_str(yaml)
        .map_err(|error| CloudError::Io(io::Error::new(io::ErrorKind::InvalidData, error)))?;
    let mut index = ManifestIndex::default();
    for (name, entry) in raw {
        let Some(entry) = entry else { continue };
        if entry.alias.is_some() {
            continue;
        }
        let Some(steam_id) = entry.steam.as_ref().and_then(|s| s.id) else {
            continue;
        };
        let game = ManifestGame {
            name,
            install_dir: entry
                .install_dir
                .as_ref()
                .and_then(|dirs| dirs.keys().next().cloned()),
            files: entry
                .files
                .iter()
                .flatten()
                .filter_map(|(pattern, file)| windows_save(pattern, file.as_ref()?))
                .collect(),
            has_registry: entry.registry.is_some(),
            steam_cloud: entry.cloud.as_ref().is_some_and(|c| c.steam),
        };
        let extra = entry.id.map(|ids| ids.steam_extra).unwrap_or_default();
        for id in std::iter::once(steam_id).chain(extra) {
            index.games.entry(id).or_insert_with(|| game.clone());
        }
    }
    Ok(index)
}

fn windows_save(pattern: &str, file: &RawFile) -> Option<ManifestFile> {
    if !file.tags.iter().any(|tag| tag == SAVE_TAG) {
        return None;
    }
    let windows: Vec<&RawWhen> = match &file.when {
        None => Vec::new(),
        Some(list) => {
            let kept: Vec<&RawWhen> = list
                .iter()
                .filter(|w| w.os.as_deref().is_none_or(|os| os == WINDOWS))
                .collect();
            if kept.is_empty() {
                return None;
            }
            kept
        }
    };
    let steam_store_only =
        !windows.is_empty() && windows.iter().all(|w| w.store.as_deref() == Some(STEAM));
    Some(ManifestFile {
        pattern: pattern.to_owned(),
        steam_store_only,
    })
}

pub fn load_index(cache_dir: &Path) -> Option<ManifestIndex> {
    read_json(&cache_dir.join(INDEX_FILE))
}

pub fn save_index(cache_dir: &Path, index: &ManifestIndex, etag: Option<&str>) -> io::Result<()> {
    write_json_atomic(&cache_dir.join(INDEX_FILE), index)?;
    match etag {
        Some(etag) => std::fs::write(cache_dir.join(ETAG_FILE), etag),
        None => Ok(()),
    }
}

pub fn cached_etag(cache_dir: &Path) -> Option<String> {
    std::fs::read_to_string(cache_dir.join(ETAG_FILE))
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

pub fn refresh(agent: &ureq::Agent, cache_dir: &Path) -> CloudResult<Refresh> {
    let etag = cached_etag(cache_dir).filter(|_| load_index(cache_dir).is_some());
    let mut request = agent.get(MANIFEST_URL);
    if let Some(etag) = &etag {
        request = request.header("If-None-Match", etag);
    }
    let mut response = request.call().map_err(network)?;
    if response.status() == 304 {
        return Ok(Refresh::NotModified);
    }
    let new_etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let text = response
        .body_mut()
        .with_config()
        .limit(MAX_MANIFEST_BYTES)
        .read_to_string()
        .map_err(network)?;
    let index = parse_manifest(&text)?;
    save_index(cache_dir, &index, new_etag.as_deref())?;
    Ok(Refresh::Updated)
}

fn network(error: ureq::Error) -> CloudError {
    CloudError::Io(io::Error::other(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../tests/ludusavi/manifest-sample.yaml");

    fn index() -> ManifestIndex {
        parse_manifest(SAMPLE).unwrap()
    }

    #[test]
    fn parses_dark_souls_iii_with_wildcard_user() {
        let index = index();
        let ds3 = &index.games[&374320];
        assert_eq!(ds3.name, "Dark Souls III");
        assert!(
            ds3.files
                .iter()
                .any(|f| f.pattern == "<winAppData>/DarkSoulsIII/<storeUserId>")
        );
        assert!(!ds3.steam_cloud);
        assert!(index.games[&1245620].steam_cloud);
        assert!(index.games.contains_key(&335300));
    }

    #[test]
    fn config_only_paths_are_skipped() {
        let index = index();
        assert!(
            index.games[&374320]
                .files
                .iter()
                .all(|f| !f.pattern.ends_with("GraphicsConfig.xml"))
        );
        let gnorp = &index.games[&1473350];
        let patterns: Vec<&str> = gnorp.files.iter().map(|f| f.pattern.as_str()).collect();
        assert_eq!(
            patterns,
            vec!["<base>/saves", "<winAppData>/Gnorp/Apologue/Saves"]
        );
        assert_eq!(gnorp.install_dir.as_deref(), Some("Gnorp"));
    }

    #[test]
    fn steam_only_when_marks_store_only_paths() {
        let index = index();
        let hack = &index.games[&525480];
        assert!(hack.files[0].steam_store_only);
        assert!(!index.games[&374320].files[0].steam_store_only);
    }

    #[test]
    fn steam_extra_ids_point_to_the_same_game() {
        let index = index();
        assert_eq!(index.games[&2778580].name, "Elden Ring");
        assert_eq!(index.games[&867990].name, "!Anyway!");
    }

    #[test]
    fn aliases_are_skipped_and_registry_is_flagged() {
        let index = index();
        assert!(index.games.values().all(|g| g.name != "-KLAUS-"));
        let registry = &index.games[&449940];
        assert!(registry.has_registry);
        assert!(registry.files.is_empty());
    }

    #[test]
    fn index_cache_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_index(dir.path()), None);
        let index = index();
        save_index(dir.path(), &index, Some("\"etag\"")).unwrap();
        assert_eq!(load_index(dir.path()), Some(index));
        assert_eq!(cached_etag(dir.path()).as_deref(), Some("\"etag\""));
    }

    #[test]
    fn invalid_yaml_is_an_error() {
        assert!(parse_manifest("- [").is_err());
    }
}
