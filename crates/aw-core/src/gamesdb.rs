use serde_json::Value;

use crate::metadata::{HTTP_TIMEOUT, MetaError, agent, http_error};
use crate::platform::{ExternalId, GameImages};
use crate::store::Store;

const BASE: &str = "https://gamesdb.gog.com";
const NOT_FOUND: u16 = 404;
const STEAM: &str = "steam";
const TITLE_KEYS: [&str; 2] = ["*", "en-US"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Found {
        title: Option<String>,
        steam_app_id: Option<u32>,
        images: GameImages,
    },
    NotFound,
}

pub fn parse_lookup(text: &str) -> Result<Lookup, MetaError> {
    let root: Value =
        serde_json::from_str(text).map_err(|error| MetaError::BadResponse(error.to_string()))?;
    let game = root.get("game");
    let title = game
        .and_then(|game| title_of(game.get("title")))
        .or_else(|| title_of(root.get("title")));
    let steam_app_id = game
        .and_then(|game| game.get("releases"))
        .and_then(Value::as_array)
        .and_then(|releases| releases.iter().find_map(steam_release));
    let image = |name: &str, ext: &str| {
        game.and_then(|game| game.get(name))
            .and_then(|image| image.get("url_format"))
            .and_then(Value::as_str)
            .map(|format| image_url(format, ext))
    };
    Ok(Lookup::Found {
        title,
        steam_app_id,
        images: GameImages {
            header: image("horizontal_artwork", "jpg"),
            hero: image("background", "jpg"),
            logo: image("logo", "png"),
            capsule: image("vertical_cover", "jpg"),
        },
    })
}

pub fn image_url(url_format: &str, ext: &str) -> String {
    url_format.replace("{formatter}", "").replace("{ext}", ext)
}

fn title_of(value: Option<&Value>) -> Option<String> {
    let value = value?;
    TITLE_KEYS
        .iter()
        .find_map(|key| value.get(key)?.as_str())
        .or_else(|| value.as_str())
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
}

fn steam_release(release: &Value) -> Option<u32> {
    if release.get("platform_id")?.as_str()? != STEAM {
        return None;
    }
    let id = release.get("external_id")?;
    id.as_str()
        .and_then(|text| text.trim().parse::<u32>().ok())
        .or_else(|| id.as_u64().and_then(|n| u32::try_from(n).ok()))
        .filter(|id| *id > 0)
}

fn encode(id: &str) -> String {
    id.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'~') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

pub fn resolve(store: &Store, db: &GamesDb, key: &ExternalId, now: i64) -> Result<u32, MetaError> {
    let (title, steam_app_id, images) = match db.lookup(key)? {
        Lookup::Found {
            title,
            steam_app_id,
            images,
        } => (title, steam_app_id, images),
        Lookup::NotFound => (None, None, GameImages::default()),
    };
    let title = title.unwrap_or_else(|| format!("{} {}", key.platform.label(), key.id));
    Ok(store.save_external_game(key, steam_app_id, Some(&title), &images, now)?)
}

pub struct GamesDb {
    agent: ureq::Agent,
    base: String,
}

impl Default for GamesDb {
    fn default() -> GamesDb {
        GamesDb::with_base(BASE)
    }
}

impl GamesDb {
    pub fn new() -> GamesDb {
        GamesDb::default()
    }

    pub fn with_base(base: &str) -> GamesDb {
        GamesDb {
            agent: agent(HTTP_TIMEOUT),
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    pub fn lookup(&self, key: &ExternalId) -> Result<Lookup, MetaError> {
        let url = format!(
            "{}/platforms/{}/external_releases/{}",
            self.base,
            key.platform.as_str(),
            encode(&key.id)
        );
        match self.agent.get(&url).call() {
            Ok(mut response) => {
                let text = response
                    .body_mut()
                    .read_to_string()
                    .map_err(|error| MetaError::Network(error.to_string()))?;
                parse_lookup(&text)
            }
            Err(ureq::Error::StatusCode(NOT_FOUND)) => Ok(Lookup::NotFound),
            Err(error) => Err(http_error(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};

    const WITH_STEAM: &str = include_str!("../tests/gamesdb/epic_with_steam.json");
    const WITHOUT_STEAM: &str = include_str!("../tests/gamesdb/gog_without_steam.json");

    fn key(platform: Platform, id: &str) -> ExternalId {
        ExternalId {
            platform,
            id: id.into(),
        }
    }

    fn server(status: &'static str, body: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                log.lock()
                    .unwrap()
                    .push(request.lines().next().unwrap_or_default().to_owned());
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        (format!("http://{addr}"), seen)
    }

    #[test]
    fn parses_first_numeric_steam_release() {
        let Lookup::Found {
            title,
            steam_app_id,
            images,
        } = parse_lookup(WITH_STEAM).unwrap()
        else {
            panic!("expected a game");
        };
        assert_eq!(title.as_deref(), Some("Grand Theft Auto V"));
        assert_eq!(steam_app_id, Some(271_590));
        assert!(images.hero.unwrap().ends_with(".jpg?namespace=gamesdb"));
        assert!(images.logo.unwrap().ends_with(".png?namespace=gamesdb"));
        assert!(images.capsule.is_some());
        assert!(images.header.is_some());
    }

    #[test]
    fn steam_release_missing_gives_none() {
        let Lookup::Found {
            title,
            steam_app_id,
            ..
        } = parse_lookup(WITHOUT_STEAM).unwrap()
        else {
            panic!("expected a game");
        };
        assert_eq!(steam_app_id, None);
        assert!(title.is_some());
    }

    #[test]
    fn skips_non_numeric_steam_ids_and_falls_back_to_release_title() {
        let text = r#"{"title":{"*":"Release"},"game":{"releases":[{"platform_id":"steam","external_id":"abc"},{"platform_id":"steam","external_id":"10"}]}}"#;
        let Lookup::Found {
            title,
            steam_app_id,
            images,
        } = parse_lookup(text).unwrap()
        else {
            panic!("expected a game");
        };
        assert_eq!(title.as_deref(), Some("Release"));
        assert_eq!(steam_app_id, Some(10));
        assert_eq!(images, GameImages::default());
    }

    #[test]
    fn bad_json_is_bad_response() {
        assert!(matches!(
            parse_lookup("nope"),
            Err(MetaError::BadResponse(_))
        ));
    }

    #[test]
    fn images_use_empty_formatter() {
        assert_eq!(
            image_url(
                "https://images.gog.com/abc{formatter}.{ext}?namespace=gamesdb",
                "jpg"
            ),
            "https://images.gog.com/abc.jpg?namespace=gamesdb"
        );
    }

    #[test]
    fn lookup_requests_platform_path() {
        let (base, seen) = server("200 OK", WITH_STEAM);
        let db = GamesDb::with_base(&base);
        let found = db.lookup(&key(Platform::Ubisoft, "4")).unwrap();
        assert!(matches!(
            found,
            Lookup::Found {
                steam_app_id: Some(271_590),
                ..
            }
        ));
        assert_eq!(
            seen.lock().unwrap()[0],
            "GET /platforms/uplay/external_releases/4 HTTP/1.1"
        );
    }

    #[test]
    fn lookup_maps_404_to_not_found() {
        let (base, _) = server("404 Not Found", "{}");
        let db = GamesDb::with_base(&base);
        assert!(matches!(
            db.lookup(&key(Platform::Gog, "1")).unwrap(),
            Lookup::NotFound
        ));
    }

    #[test]
    fn lookup_errors_on_500() {
        let (base, _) = server("500 Internal Server Error", "{}");
        let db = GamesDb::with_base(&base);
        assert!(db.lookup(&key(Platform::Epic, "X")).is_err());
    }

    #[test]
    fn resolve_saves_steam_id_and_title() {
        let (base, _) = server("200 OK", WITH_STEAM);
        let store = crate::store::Store::in_memory().unwrap();
        let k = key(Platform::Epic, "9d2d0eb64d5c44529cece33fe2a46482");
        let app_id = resolve(&store, &GamesDb::with_base(&base), &k, 7).unwrap();
        assert_eq!(app_id, 271_590);
        assert_eq!(store.external_app_id(&k).unwrap(), Some(271_590));
        assert_eq!(
            store.game_name(271_590).unwrap().as_deref(),
            Some("Grand Theft Auto V")
        );
    }

    #[test]
    fn resolve_not_found_uses_synthetic_with_platform_label() {
        let (base, _) = server("404 Not Found", "{}");
        let store = crate::store::Store::in_memory().unwrap();
        let k = key(Platform::Epic, "X");
        let app_id = resolve(&store, &GamesDb::with_base(&base), &k, 7).unwrap();
        assert!(crate::platform::is_synthetic(app_id));
        assert_eq!(store.game_name(app_id).unwrap().as_deref(), Some("Epic X"));
    }

    #[test]
    fn resolve_network_error_saves_nothing() {
        let store = crate::store::Store::in_memory().unwrap();
        let k = key(Platform::Gog, "1");
        assert!(resolve(&store, &GamesDb::with_base("http://127.0.0.1:9"), &k, 7).is_err());
        assert_eq!(store.external_app_id(&k).unwrap(), None);
    }

    #[test]
    fn rune_file_resolves_to_steam_game_end_to_end() {
        use crate::discovery::folders::KnownFolders;
        use crate::discovery::lumaplay::MemoryRegistry;
        use crate::engine::{CoreSettings, Engine};
        use crate::store::{SchemaRow, Store};

        const NOW: i64 = 1_800_000_000;
        const BODY: &str = r#"{"title":{"*":"Assassin's Creed II"},"game":{"releases":[{"platform_id":"uplay","external_id":"4"},{"platform_id":"steam","external_id":"33230"}]}}"#;
        let (base, _) = server("200 OK", BODY);
        let tmp = tempfile::tempdir().unwrap();
        let cfg = tmp
            .path()
            .join("Documents/RUNE/Ubisoft Connect/achievements/player/4/achievements.cfg");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(
            &cfg,
            r#""achievements" { "1" { "earned" "1" "time" "1700000000" } }"#,
        )
        .unwrap();
        let mut engine = Engine::new(
            Store::in_memory().unwrap(),
            KnownFolders::under(tmp.path()),
            CoreSettings::default(),
        );
        engine.set_registry(std::sync::Arc::new(MemoryRegistry::default()));
        let out = engine.sync(NOW, true).unwrap();
        assert_eq!(out.pending, vec![key(Platform::Ubisoft, "4")]);
        let app_id = resolve(
            engine.store(),
            &GamesDb::with_base(&base),
            &out.pending[0],
            NOW,
        )
        .unwrap();
        assert_eq!(app_id, 33230);
        let row = |name: &str| SchemaRow {
            api_name: name.into(),
            display_name: name.into(),
            description: String::new(),
            icon: None,
            icon_gray: None,
            hidden: false,
            percent: None,
        };
        engine
            .store()
            .replace_schema(
                33230,
                "english",
                Some("Assassin's Creed II"),
                &[row("ACH_ENTER_ANIMUS_1"), row("ACH_LEAVE_ANIMUS_2")],
                NOW,
            )
            .unwrap();
        let out = engine.sync(NOW + 1, false).unwrap();
        let got: Vec<(u32, &str)> = out
            .new_unlocks
            .iter()
            .map(|u| (u.app_id, u.api_name.as_str()))
            .collect();
        assert_eq!(got, vec![(33230, "ACH_ENTER_ANIMUS_1")]);
        assert_eq!(
            engine.store().game_name(33230).unwrap().as_deref(),
            Some("Assassin's Creed II")
        );
    }
}
