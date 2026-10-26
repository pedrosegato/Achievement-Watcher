use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::platform::is_synthetic;
use crate::store::{SchemaRow, Store, StoreError};

pub const CACHE_TTL_SECS: i64 = 30 * 24 * 60 * 60;
pub(crate) const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum MetaError {
    #[error("network: {0}")]
    Network(String),
    #[error("http status {0}")]
    Status(u16),
    #[error("bad response: {0}")]
    BadResponse(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ensured {
    Fresh,
    Fetched,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub api: String,
    pub store: String,
    pub cdn: String,
    pub community: String,
    pub assets: String,
}

impl Default for Endpoints {
    fn default() -> Endpoints {
        Endpoints {
            api: "https://api.steampowered.com".into(),
            store: "https://store.steampowered.com".into(),
            cdn: "https://cdn.akamai.steamstatic.com".into(),
            community: "https://shared.akamai.steamstatic.com/community_assets/images/apps".into(),
            assets: "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps".into(),
        }
    }
}

pub struct SteamMeta {
    agent: ureq::Agent,
    endpoints: Endpoints,
    cache_dir: PathBuf,
    lang: String,
}

#[derive(Deserialize)]
struct AchievementsEnvelope {
    response: AchievementsResponse,
}

#[derive(Deserialize)]
struct AchievementsResponse {
    #[serde(default)]
    achievements: Vec<RawAchievement>,
}

#[derive(Deserialize)]
struct RawAchievement {
    internal_name: String,
    #[serde(default)]
    localized_name: String,
    #[serde(default)]
    localized_desc: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    icon_gray: Option<String>,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    player_percent_unlocked: Option<String>,
}

pub fn parse_achievements_json(text: &str) -> Result<Vec<SchemaRow>, MetaError> {
    let envelope: AchievementsEnvelope =
        serde_json::from_str(text).map_err(|e| MetaError::BadResponse(e.to_string()))?;
    Ok(envelope
        .response
        .achievements
        .into_iter()
        .map(|raw| SchemaRow {
            display_name: if raw.localized_name.is_empty() {
                raw.internal_name.clone()
            } else {
                raw.localized_name
            },
            api_name: raw.internal_name,
            description: raw.localized_desc,
            icon: raw.icon.filter(|s| is_plain_file_name(s)),
            icon_gray: raw.icon_gray.filter(|s| is_plain_file_name(s)),
            hidden: raw.hidden,
            percent: raw
                .player_percent_unlocked
                .and_then(|p| p.trim().parse::<f64>().ok()),
        })
        .collect())
}

fn appdetails_field(text: &str, app_id: u32, field: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let entry = value.get(app_id.to_string())?;
    if entry.get("success")?.as_bool()? {
        entry
            .get("data")?
            .get(field)?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    } else {
        None
    }
}

pub fn parse_appdetails_name(text: &str, app_id: u32) -> Option<String> {
    appdetails_field(text, app_id, "name")
}

pub fn parse_appdetails_header(text: &str, app_id: u32) -> Option<String> {
    appdetails_field(text, app_id, "header_image")
}

pub(crate) fn http_error(error: ureq::Error) -> MetaError {
    match error {
        ureq::Error::StatusCode(code) => MetaError::Status(code),
        other => MetaError::Network(other.to_string()),
    }
}

pub(crate) fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent("AchievementWatcher/0.1")
        .build()
        .into()
}

fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
        && !name.contains("..")
}

impl SteamMeta {
    pub fn new(cache_dir: PathBuf, lang: &str) -> SteamMeta {
        SteamMeta::with_endpoints(cache_dir, lang, Endpoints::default())
    }

    pub fn with_endpoints(cache_dir: PathBuf, lang: &str, endpoints: Endpoints) -> SteamMeta {
        SteamMeta {
            agent: agent(HTTP_TIMEOUT),
            endpoints,
            cache_dir,
            lang: lang.to_owned(),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> SteamMeta {
        self.agent = agent(timeout);
        self
    }

    pub fn lang(&self) -> &str {
        &self.lang
    }

    pub fn icon_path(&self, app_id: u32, file: &str) -> PathBuf {
        self.cache_dir
            .join("icons")
            .join(app_id.to_string())
            .join(file)
    }

    pub fn icon_urls(&self, app_id: u32, file: &str) -> [String; 2] {
        [
            format!("{}/{app_id}/{file}", self.endpoints.community),
            format!(
                "{}/steamcommunity/public/images/apps/{app_id}/{file}",
                self.endpoints.cdn
            ),
        ]
    }

    pub fn asset_url(&self, app_id: u32, file: &str) -> String {
        format!("{}/{app_id}/{file}", self.endpoints.assets)
    }

    pub fn appdetails_header(&self, app_id: u32) -> Result<Option<String>, MetaError> {
        let url = format!(
            "{}/api/appdetails?appids={app_id}&filters=basic",
            self.endpoints.store
        );
        Ok(parse_appdetails_header(&self.get_text(&url)?, app_id))
    }

    pub fn ensure(&self, store: &Store, app_id: u32, now: i64) -> Result<Ensured, MetaError> {
        if is_synthetic(app_id) {
            return Ok(Ensured::Fresh);
        }
        let cached = store.schema_info(app_id)?;
        if let Some((lang, fetched_at)) = &cached
            && *lang == self.lang
            && now - fetched_at < CACHE_TTL_SECS
        {
            return Ok(Ensured::Fresh);
        }
        match self.fetch(app_id) {
            Ok((name, rows)) => {
                store.replace_schema(app_id, &self.lang, name.as_deref(), &rows, now)?;
                Ok(Ensured::Fetched)
            }
            Err(error) if cached.is_some() => {
                tracing::warn!("using stale metadata for {app_id}: {error}");
                Ok(Ensured::Stale)
            }
            Err(error) => Err(error),
        }
    }

    fn fetch(&self, app_id: u32) -> Result<(Option<String>, Vec<SchemaRow>), MetaError> {
        let url = format!(
            "{}/IPlayerService/GetGameAchievements/v1/?appid={app_id}&language={}",
            self.endpoints.api, self.lang
        );
        let rows = parse_achievements_json(&self.get_text(&url)?)?;
        let details = format!(
            "{}/api/appdetails?appids={app_id}&filters=basic",
            self.endpoints.store
        );
        let name = self
            .get_text(&details)
            .ok()
            .and_then(|text| parse_appdetails_name(&text, app_id));
        Ok((name, rows))
    }

    fn get_text(&self, url: &str) -> Result<String, MetaError> {
        self.agent
            .get(url)
            .call()
            .map_err(http_error)?
            .body_mut()
            .read_to_string()
            .map_err(|e| MetaError::Network(e.to_string()))
    }

    pub fn ensure_icon(&self, app_id: u32, file: &str) -> bool {
        if is_synthetic(app_id) {
            return false;
        }
        let target = self.icon_path(app_id, file);
        if target.is_file() {
            return true;
        }
        let mut last = None;
        for url in self.icon_urls(app_id, file) {
            match self.download(&url, &target) {
                Ok(()) => return true,
                Err(error) => last = Some(error),
            }
        }
        if let Some(error) = last {
            tracing::warn!("icon {file} for {app_id}: {error}");
        }
        false
    }

    pub fn ensure_icons(&self, store: &Store, app_id: u32) -> Result<(), MetaError> {
        if is_synthetic(app_id) {
            return Ok(());
        }
        let mut files: Vec<String> = store
            .schema_icons(app_id)?
            .into_iter()
            .filter(|file| !self.icon_path(app_id, file).is_file())
            .collect();
        files.sort_unstable();
        files.dedup();
        for file in files {
            self.ensure_icon(app_id, &file);
        }
        Ok(())
    }

    pub fn download(&self, url: &str, target: &Path) -> Result<(), MetaError> {
        let bytes = self
            .agent
            .get(url)
            .call()
            .map_err(http_error)?
            .body_mut()
            .read_to_vec()
            .map_err(|e| MetaError::Network(e.to_string()))?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| MetaError::Network(e.to_string()))?;
        }
        let temp = target.with_extension("part");
        std::fs::write(&temp, bytes).map_err(|e| MetaError::Network(e.to_string()))?;
        std::fs::rename(&temp, target).map_err(|e| MetaError::Network(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
  "response": {
    "achievements": [
      {
        "internal_name": "ACH00",
        "localized_name": "Elden Ring",
        "localized_desc": "Obtained all achievements",
        "icon": "1cb1fe89084e822bd7d9ad4df66bc7f917995b38.jpg",
        "icon_gray": "172885dcb6a82300de9f34e8833afdf7d7cd24e1.jpg",
        "hidden": false,
        "player_percent_unlocked": "10.5",
        "internal_key": 256,
        "groupid": 0,
        "archived": false,
        "progress_type": 0
      },
      {
        "internal_name": "ACH01",
        "localized_name": "Elden Lord",
        "localized_desc": "Achieved the \"Elden Lord\" ending",
        "icon": "e50726be4aca4d2b1f5f14d36859fc8e6f710097.jpg",
        "icon_gray": "172885dcb6a82300de9f34e8833afdf7d7cd24e1.jpg",
        "hidden": true,
        "player_percent_unlocked": "23.9",
        "internal_key": 257,
        "groupid": 0,
        "archived": false,
        "progress_type": 0
      },
      {
        "internal_name": "ACH02",
        "localized_name": "Age of the Stars",
        "localized_desc": "Achieved the \"Age of the Stars\" ending",
        "icon": "7e7369bceba13739025766a72560fe9ce7ac56c9.jpg",
        "icon_gray": "172885dcb6a82300de9f34e8833afdf7d7cd24e1.jpg",
        "hidden": true,
        "player_percent_unlocked": "28.2",
        "internal_key": 258,
        "groupid": 0,
        "archived": false,
        "progress_type": 0
      }
    ],
    "schema_version": 47,
    "groups": [
      {
        "groupid": 0,
        "localized_name": "",
        "dlcappid": 0,
        "archived": false,
        "developeronly": false,
        "order": 0,
        "ispublic": true,
        "total_achievements": 42,
        "completion_achievements": 42
      }
    ],
    "schema_hash": 961205965
  }
}"#;

    fn offline(cache: &Path) -> SteamMeta {
        SteamMeta::with_endpoints(
            cache.to_path_buf(),
            "english",
            Endpoints {
                api: "http://127.0.0.1:9".into(),
                store: "http://127.0.0.1:9".into(),
                cdn: "http://127.0.0.1:9".into(),
                community: "http://127.0.0.1:9/community_assets/images/apps".into(),
                assets: "http://127.0.0.1:9".into(),
            },
        )
    }

    fn serve_once_per_path(hits: usize) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(hits) {
                let mut stream = stream.unwrap();
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let reply = if request.contains("/community_assets/") {
                    "HTTP/1.1 404 Not Found
Content-Length: 0
Connection: close

"
                    .to_string()
                } else {
                    "HTTP/1.1 200 OK
Content-Length: 4
Connection: close

icon"
                        .to_string()
                };
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn icon_urls_try_community_assets_first() {
        let meta = SteamMeta::new(PathBuf::from("."), "english");
        let file = "4031100910abf99aa0482464a06991bc9273e417.jpg";
        assert_eq!(
            meta.icon_urls(4001890, file),
            [
                format!(
                    "https://shared.akamai.steamstatic.com/community_assets/images/apps/4001890/{file}"
                ),
                format!(
                    "https://cdn.akamai.steamstatic.com/steamcommunity/public/images/apps/4001890/{file}"
                ),
            ]
        );
    }

    #[test]
    fn ensure_icon_falls_back_to_legacy_cdn() {
        let tmp = tempfile::tempdir().unwrap();
        let base = serve_once_per_path(2);
        let meta = SteamMeta::with_endpoints(
            tmp.path().to_path_buf(),
            "english",
            Endpoints {
                api: base.clone(),
                store: base.clone(),
                cdn: base.clone(),
                community: format!("{base}/community_assets/images/apps"),
                assets: base.clone(),
            },
        );
        assert!(meta.ensure_icon(4001890, "abc.jpg"));
        assert_eq!(
            std::fs::read(meta.icon_path(4001890, "abc.jpg")).unwrap(),
            b"icon"
        );
    }

    #[test]
    fn parses_game_achievements_response() {
        let rows = parse_achievements_json(SAMPLE).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].api_name, "ACH00");
        assert_eq!(rows[0].display_name, "Elden Ring");
        assert_eq!(
            rows[0].icon.as_deref(),
            Some("1cb1fe89084e822bd7d9ad4df66bc7f917995b38.jpg")
        );
        assert_eq!(rows[0].percent, Some(10.5));
        assert!(rows[1].hidden);
    }

    #[test]
    fn rejects_icon_names_with_paths() {
        let rows = parse_achievements_json(
            r#"{"response":{"achievements":[{"internal_name":"A","icon":"../x.jpg","icon_gray":"a/b.jpg"}]}}"#,
        )
        .unwrap();
        assert_eq!(rows[0].icon, None);
        assert_eq!(rows[0].icon_gray, None);
        assert_eq!(rows[0].display_name, "A");
    }

    #[test]
    fn empty_response_has_no_achievements() {
        assert!(
            parse_achievements_json(r#"{"response":{}}"#)
                .unwrap()
                .is_empty()
        );
        assert!(parse_achievements_json("not json").is_err());
    }

    #[test]
    fn appdetails_header_with_hash_is_parsed() {
        let url = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/3678970/b817d4ab0e3c1c5f/header_alt_assets_1.jpg?t=1761000000";
        let text = format!(
            r#"{{"3678970":{{"success":true,"data":{{"name":"X","header_image":"{url}"}}}}}}"#
        );
        assert_eq!(
            parse_appdetails_header(&text, 3678970).as_deref(),
            Some(url)
        );
        assert_eq!(
            parse_appdetails_header(r#"{"5":{"success":false}}"#, 5),
            None
        );
    }

    #[test]
    fn asset_urls_use_store_item_assets() {
        let meta = SteamMeta::new(PathBuf::from("."), "english");
        assert_eq!(
            meta.asset_url(413150, "library_hero.jpg"),
            "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/413150/library_hero.jpg"
        );
    }

    #[test]
    fn parses_appdetails_name() {
        assert_eq!(
            parse_appdetails_name(
                r#"{"1245620":{"success":true,"data":{"name":"ELDEN RING"}}}"#,
                1245620
            ),
            Some("ELDEN RING".into())
        );
        assert_eq!(parse_appdetails_name(r#"{"5":{"success":false}}"#, 5), None);
    }

    #[test]
    fn stale_cache_is_used_when_offline() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::in_memory().unwrap();
        let rows = parse_achievements_json(SAMPLE).unwrap();
        store
            .replace_schema(7, "english", Some("Game"), &rows, 0)
            .unwrap();
        let meta = offline(tmp.path());
        assert_eq!(
            meta.ensure(&store, 7, CACHE_TTL_SECS * 2).unwrap(),
            Ensured::Stale
        );
        assert_eq!(
            store.schema_row(7, "ACH00").unwrap().unwrap().display_name,
            "Elden Ring"
        );
    }

    #[test]
    fn fresh_cache_skips_network() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::in_memory().unwrap();
        store.replace_schema(7, "english", None, &[], 100).unwrap();
        let meta = offline(tmp.path());
        assert_eq!(meta.ensure(&store, 7, 200).unwrap(), Ensured::Fresh);
    }

    #[test]
    fn missing_cache_and_no_network_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::in_memory().unwrap();
        assert!(offline(tmp.path()).ensure(&store, 7, 200).is_err());
    }

    #[test]
    fn synthetic_ids_never_reach_steam() {
        let cache = tempfile::tempdir().unwrap();
        let closed = Endpoints {
            api: "http://127.0.0.1:9".into(),
            store: "http://127.0.0.1:9".into(),
            cdn: "http://127.0.0.1:9".into(),
            community: "http://127.0.0.1:9".into(),
            assets: "http://127.0.0.1:9".into(),
        };
        let meta = SteamMeta::with_endpoints(cache.path().to_path_buf(), "english", closed);
        let store = Store::in_memory().unwrap();
        let synthetic = crate::platform::SYNTHETIC_BASE + 5;
        assert_eq!(meta.ensure(&store, synthetic, 1).unwrap(), Ensured::Fresh);
        assert!(!meta.ensure_icon(synthetic, "a.jpg"));
        meta.ensure_icons(&store, synthetic).unwrap();
        assert_eq!(store.schema_info(synthetic).unwrap(), None);
    }
}
