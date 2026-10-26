use std::path::{Path, PathBuf};

use aw_core::metadata::{MetaError, SteamMeta};
use aw_core::platform::{GameImages, is_synthetic};
use serde::{Deserialize, Serialize};

use crate::popup::PROTOCOL_ORIGIN;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtKind {
    Header,
    Hero,
    Logo,
    Capsule,
}

const FETCHED: [ArtKind; 3] = [ArtKind::Header, ArtKind::Hero, ArtKind::Logo];
const EXTERNAL: [ArtKind; 4] = [
    ArtKind::Header,
    ArtKind::Hero,
    ArtKind::Logo,
    ArtKind::Capsule,
];

impl ArtKind {
    pub fn name(self) -> &'static str {
        match self {
            ArtKind::Header => "header",
            ArtKind::Hero => "hero",
            ArtKind::Logo => "logo",
            ArtKind::Capsule => "capsule",
        }
    }

    pub fn from_name(name: &str) -> Option<ArtKind> {
        [
            ArtKind::Header,
            ArtKind::Hero,
            ArtKind::Logo,
            ArtKind::Capsule,
        ]
        .into_iter()
        .find(|kind| kind.name() == name)
    }

    pub fn file_name(self) -> &'static str {
        match self {
            ArtKind::Header => "header.jpg",
            ArtKind::Hero => "library_hero.jpg",
            ArtKind::Logo => "logo.png",
            ArtKind::Capsule => "library_600x900.jpg",
        }
    }

    fn local_names(self) -> &'static [&'static str] {
        match self {
            ArtKind::Header => &["header.jpg", "library_header.jpg"],
            ArtKind::Hero => &["library_hero.jpg"],
            ArtKind::Logo => &["logo.png"],
            ArtKind::Capsule => &["library_600x900.jpg", "library_capsule.jpg"],
        }
    }
}

pub fn art_url(app_id: u32, kind: ArtKind) -> String {
    format!("{PROTOCOL_ORIGIN}/art/{app_id}/{}", kind.name())
}

pub fn librarycache_file(steam: &Path, app_id: u32, kind: ArtKind) -> Option<PathBuf> {
    let dir = steam
        .join("appcache")
        .join("librarycache")
        .join(app_id.to_string());
    let names = kind.local_names();
    if let Some(direct) = names
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
    {
        return Some(direct);
    }
    std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .flat_map(|sub| names.iter().map(move |name| sub.join(name)))
        .find(|path| path.is_file())
}

fn art_dir(cache_dir: &Path, app_id: u32) -> PathBuf {
    cache_dir.join("art").join(app_id.to_string())
}

fn marker(cache_dir: &Path, app_id: u32, kind: ArtKind) -> PathBuf {
    art_dir(cache_dir, app_id).join(format!(".missing-{}", kind.name()))
}

pub fn cached_file(cache_dir: &Path, app_id: u32, kind: ArtKind) -> Option<PathBuf> {
    Some(art_dir(cache_dir, app_id).join(kind.file_name())).filter(|path| path.is_file())
}

pub fn resolve_art(
    steam: Option<&Path>,
    cache_dir: &Path,
    app_id: u32,
    kind: ArtKind,
) -> Option<PathBuf> {
    steam
        .and_then(|steam| librarycache_file(steam, app_id, kind))
        .or_else(|| cached_file(cache_dir, app_id, kind))
}

pub fn versioned_url(steam: Option<&Path>, cache_dir: &Path, app_id: u32, kind: ArtKind) -> String {
    let url = art_url(app_id, kind);
    let stamp = resolve_art(steam, cache_dir, app_id, kind)
        .and_then(|path| std::fs::metadata(path).ok())
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|age| age.as_secs());
    match stamp {
        Some(stamp) => format!("{url}?v={stamp}"),
        None => url,
    }
}

fn wanted(steam: Option<&Path>, cache_dir: &Path, app_id: u32, kind: ArtKind) -> bool {
    resolve_art(steam, cache_dir, app_id, kind).is_none()
        && !marker(cache_dir, app_id, kind).exists()
}

pub fn needs_fetch(steam: Option<&Path>, cache_dir: &Path, app_id: u32) -> bool {
    FETCHED
        .into_iter()
        .any(|kind| wanted(steam, cache_dir, app_id, kind))
}

fn fetch_one(
    meta: &SteamMeta,
    cache_dir: &Path,
    app_id: u32,
    kind: ArtKind,
) -> Result<bool, MetaError> {
    let url = match kind {
        ArtKind::Header => match meta.appdetails_header(app_id) {
            Ok(url) => url,
            Err(MetaError::Status(404)) => None,
            Err(error) => return Err(error),
        },
        _ => Some(meta.asset_url(app_id, kind.file_name())),
    };
    let Some(url) = url else {
        return Ok(false);
    };
    match meta.download(&url, &art_dir(cache_dir, app_id).join(kind.file_name())) {
        Ok(()) => Ok(true),
        Err(MetaError::Status(404)) => Ok(false),
        Err(error) => Err(error),
    }
}

pub fn fetch_art(meta: &SteamMeta, steam: Option<&Path>, cache_dir: &Path, app_id: u32) -> bool {
    if is_synthetic(app_id) {
        return false;
    }
    fetch_kinds(steam, cache_dir, app_id, &FETCHED, |kind| {
        fetch_one(meta, cache_dir, app_id, kind)
    })
}

pub fn fetch_external_art(
    meta: &SteamMeta,
    cache_dir: &Path,
    app_id: u32,
    images: &GameImages,
) -> bool {
    fetch_kinds(None, cache_dir, app_id, &EXTERNAL, |kind| {
        let Some(url) = external_url(images, kind) else {
            return Ok(false);
        };
        match meta.download(url, &art_dir(cache_dir, app_id).join(kind.file_name())) {
            Ok(()) => Ok(true),
            Err(MetaError::Status(404)) => Ok(false),
            Err(error) => Err(error),
        }
    })
}

fn external_url(images: &GameImages, kind: ArtKind) -> Option<&str> {
    match kind {
        ArtKind::Header => images.header.as_deref(),
        ArtKind::Hero => images.hero.as_deref(),
        ArtKind::Logo => images.logo.as_deref(),
        ArtKind::Capsule => images.capsule.as_deref(),
    }
}

fn fetch_kinds(
    steam: Option<&Path>,
    cache_dir: &Path,
    app_id: u32,
    kinds: &[ArtKind],
    fetch: impl Fn(ArtKind) -> Result<bool, MetaError>,
) -> bool {
    let mut fetched = false;
    for &kind in kinds {
        if !wanted(steam, cache_dir, app_id, kind) {
            continue;
        }
        match fetch(kind) {
            Ok(true) => fetched = true,
            Ok(false) => {
                let path = marker(cache_dir, app_id, kind);
                let written = std::fs::create_dir_all(art_dir(cache_dir, app_id))
                    .and_then(|()| std::fs::write(&path, b""));
                if let Err(error) = written {
                    tracing::warn!(
                        "cannot mark {} art missing for {app_id}: {error}",
                        kind.name()
                    );
                }
            }
            Err(error) => tracing::warn!("{} art for {app_id}: {error}", kind.name()),
        }
    }
    fetched
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use aw_core::metadata::{Endpoints, SteamMeta};

    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"img").unwrap();
    }

    fn endpoints(base: &str) -> Endpoints {
        Endpoints {
            api: base.into(),
            store: base.into(),
            cdn: base.into(),
            community: base.into(),
            assets: base.into(),
        }
    }

    fn not_found_server() -> (String, Arc<AtomicUsize>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                counter.fetch_add(1, Ordering::SeqCst);
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        });
        (format!("http://{addr}"), hits)
    }

    #[test]
    fn librarycache_finds_root_and_hash_subfolder() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("appcache").join("librarycache");
        touch(&cache.join("413150").join("header.jpg"));
        touch(&cache.join("3678970").join("b817d4ab").join("header.jpg"));
        assert_eq!(
            librarycache_file(tmp.path(), 413150, ArtKind::Header),
            Some(cache.join("413150").join("header.jpg"))
        );
        assert_eq!(
            librarycache_file(tmp.path(), 3678970, ArtKind::Header),
            Some(cache.join("3678970").join("b817d4ab").join("header.jpg"))
        );
        assert_eq!(librarycache_file(tmp.path(), 413150, ArtKind::Logo), None);
    }

    #[test]
    fn librarycache_accepts_current_steam_names() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("appcache").join("librarycache");
        touch(&cache.join("250900").join("library_header.jpg"));
        touch(
            &cache
                .join("1091500")
                .join("812a")
                .join("library_header.jpg"),
        );
        touch(
            &cache
                .join("1091500")
                .join("6399")
                .join("library_capsule.jpg"),
        );
        assert_eq!(
            librarycache_file(tmp.path(), 250900, ArtKind::Header),
            Some(cache.join("250900").join("library_header.jpg"))
        );
        assert_eq!(
            librarycache_file(tmp.path(), 1091500, ArtKind::Header),
            Some(
                cache
                    .join("1091500")
                    .join("812a")
                    .join("library_header.jpg")
            )
        );
        assert_eq!(
            librarycache_file(tmp.path(), 1091500, ArtKind::Capsule),
            Some(
                cache
                    .join("1091500")
                    .join("6399")
                    .join("library_capsule.jpg")
            )
        );
    }

    #[test]
    fn versioned_url_changes_once_art_exists() {
        let cache = tempfile::tempdir().unwrap();
        let plain = art_url(7, ArtKind::Header);
        assert_eq!(versioned_url(None, cache.path(), 7, ArtKind::Header), plain);
        touch(&cache.path().join("art").join("7").join("header.jpg"));
        let versioned = versioned_url(None, cache.path(), 7, ArtKind::Header);
        assert!(versioned.starts_with(&format!("{plain}?v=")), "{versioned}");
    }

    #[test]
    fn resolve_art_prefers_local_over_cache() {
        let steam = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let local = steam
            .path()
            .join("appcache")
            .join("librarycache")
            .join("413150")
            .join("library_hero.jpg");
        let cached_hero = cache
            .path()
            .join("art")
            .join("413150")
            .join("library_hero.jpg");
        let cached_logo = cache.path().join("art").join("413150").join("logo.png");
        touch(&local);
        touch(&cached_hero);
        touch(&cached_logo);
        assert_eq!(
            resolve_art(Some(steam.path()), cache.path(), 413150, ArtKind::Hero),
            Some(local)
        );
        assert_eq!(
            resolve_art(Some(steam.path()), cache.path(), 413150, ArtKind::Logo),
            Some(cached_logo)
        );
    }

    #[test]
    fn resolve_art_returns_none_when_nothing_found() {
        let steam = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        assert_eq!(resolve_art(None, cache.path(), 1, ArtKind::Header), None);
        assert_eq!(
            resolve_art(Some(steam.path()), cache.path(), 1, ArtKind::Capsule),
            None
        );
    }

    #[test]
    fn missing_marker_prevents_refetch() {
        let (base, hits) = not_found_server();
        let cache = tempfile::tempdir().unwrap();
        let meta =
            SteamMeta::with_endpoints(cache.path().to_path_buf(), "english", endpoints(&base));
        fetch_art(&meta, None, cache.path(), 413150);
        assert_eq!(hits.load(Ordering::SeqCst), 3);
        for kind in [ArtKind::Header, ArtKind::Hero, ArtKind::Logo] {
            let marker = cache
                .path()
                .join("art")
                .join("413150")
                .join(format!(".missing-{}", kind.name()));
            assert!(marker.is_file(), "{marker:?}");
        }
        fetch_art(&meta, None, cache.path(), 413150);
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn network_failure_leaves_no_marker() {
        let cache = tempfile::tempdir().unwrap();
        let meta = SteamMeta::with_endpoints(
            cache.path().to_path_buf(),
            "english",
            endpoints("http://127.0.0.1:9"),
        );
        fetch_art(&meta, None, cache.path(), 413150);
        assert!(
            !cache
                .path()
                .join("art")
                .join("413150")
                .join(".missing-hero")
                .exists()
        );
    }

    #[test]
    fn kind_names_round_trip() {
        for kind in [
            ArtKind::Header,
            ArtKind::Hero,
            ArtKind::Logo,
            ArtKind::Capsule,
        ] {
            assert_eq!(ArtKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(ArtKind::Capsule.file_name(), "library_600x900.jpg");
        assert_eq!(ArtKind::from_name("banner"), None);
    }

    fn ok_server() -> (String, Arc<AtomicUsize>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                counter.fetch_add(1, Ordering::SeqCst);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: 3\r\nConnection: close\r\n\r\nimg",
                );
            }
        });
        (format!("http://{addr}"), hits)
    }

    #[test]
    fn synthetic_ids_skip_steam_art() {
        let (base, hits) = not_found_server();
        let cache = tempfile::tempdir().unwrap();
        let meta =
            SteamMeta::with_endpoints(cache.path().to_path_buf(), "english", endpoints(&base));
        assert!(!fetch_art(&meta, None, cache.path(), 0x8000_0001));
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        assert!(!cache.path().join("art").join("2147483649").exists());
    }

    #[test]
    fn external_art_downloads_mapped_files() {
        let (base, hits) = ok_server();
        let cache = tempfile::tempdir().unwrap();
        let meta = SteamMeta::new(cache.path().to_path_buf(), "english");
        let images = aw_core::platform::GameImages {
            header: Some(format!("{base}/h.jpg")),
            hero: Some(format!("{base}/b.jpg")),
            logo: None,
            capsule: Some(format!("{base}/c.jpg")),
        };
        assert!(fetch_external_art(
            &meta,
            cache.path(),
            0x8000_0001,
            &images
        ));
        let dir = cache.path().join("art").join("2147483649");
        assert!(dir.join("header.jpg").is_file());
        assert!(dir.join("library_hero.jpg").is_file());
        assert!(dir.join("library_600x900.jpg").is_file());
        assert!(dir.join(".missing-logo").is_file());
        assert_eq!(hits.load(Ordering::SeqCst), 3);
        assert!(!fetch_external_art(
            &meta,
            cache.path(),
            0x8000_0001,
            &images
        ));
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }
}
