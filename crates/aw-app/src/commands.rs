use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use aw_core::discovery::appinfo::{LogoPosition, read_logo_position};
use aw_core::discovery::gamediscovery::Unresolved;
use aw_core::discovery::lumaplay::WindowsRegistry;
use aw_core::discovery::sources::builtin_sources;
use aw_core::discovery::steam::steam_install_path;
use aw_core::platform::ExternalId;
use aw_core::store::{AchievementView, ExternalGame, FileReport, GameOverview};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::app::{self, AppState};
use crate::art::{ArtKind, art_url, versioned_url};
use crate::diag;
use crate::popup::{PROTOCOL_ORIGIN, PopupKind};
use crate::presets::{self, PresetInfo};
use crate::report;
use crate::settings::Settings;
use crate::sound;
use crate::sounds::{self, SoundEvent, SoundPack};

const CDN: &str = "https://cdn.akamai.steamstatic.com";
const EXTRA_SOURCE_NAMES: [(&str, &str); 2] = [
    ("lumaplay", "LumaPlay"),
    ("universelan_game", "UniverseLAN"),
];
const COMMUNITY: &str = "https://shared.akamai.steamstatic.com/community_assets/images/apps";
const STEAM_SOURCE: &str = "steam";
pub const RECENT_ICONS: usize = 6;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRow {
    pub app_id: u32,
    pub name: String,
    pub unlocked: u32,
    pub total: u32,
    pub last_unlock_at: Option<i64>,
    pub cover_url: String,
    pub playtime_seconds: i64,
    pub last_played: Option<i64>,
    pub playing: bool,
    pub origin: Option<String>,
    pub origin_id: Option<String>,
    pub header_url: String,
    pub platinum_at: Option<i64>,
    pub recent: Vec<RecentIcon>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentIcon {
    pub api_name: String,
    pub display_name: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub percent: Option<f64>,
    pub unlocked_at: i64,
    pub playtime_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AchievementRow {
    pub api_name: String,
    pub display_name: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub icon_gray_url: Option<String>,
    pub hidden: bool,
    pub percent: Option<f64>,
    pub unlocked_at: Option<i64>,
    pub playtime_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDetail {
    pub app_id: u32,
    pub name: String,
    pub cover_url: String,
    pub unlocked: u32,
    pub total: u32,
    pub playtime_seconds: i64,
    pub playing: bool,
    pub origin: Option<String>,
    pub header_url: String,
    pub hero_url: String,
    pub logo_url: String,
    pub logo_position: Option<LogoPosition>,
    pub platinum_at: Option<i64>,
    pub achievements: Vec<AchievementRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRow {
    pub id: String,
    pub name: String,
    pub root: Option<String>,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub version: String,
    pub data_dir: String,
    pub language: String,
    pub last_sync_at: Option<i64>,
    pub sources: Vec<SourceRow>,
    pub watched: Vec<String>,
    pub unresolved: Vec<Unresolved>,
    pub pending: Vec<ExternalId>,
    pub external: Vec<ExternalGame>,
    pub files: Vec<FileReport>,
    pub log_tail: Vec<String>,
}

pub fn cover_url(app_id: u32) -> String {
    format!("{CDN}/steam/apps/{app_id}/header.jpg")
}

fn display_name(app_id: u32, name: Option<String>) -> String {
    name.filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("App {app_id}"))
}

pub fn origin_name(source_id: &str) -> String {
    if source_id == STEAM_SOURCE {
        return "Steam".into();
    }
    if let Some((_, name)) = EXTRA_SOURCE_NAMES.iter().find(|(id, _)| *id == source_id) {
        return (*name).into();
    }
    builtin_sources()
        .iter()
        .find(|source| source.id == source_id)
        .map_or_else(|| source_id.to_owned(), |source| source.name.to_owned())
}

pub fn game_rows(
    overview: Vec<GameOverview>,
    playing: &[u32],
    recent: Vec<(u32, RecentIcon)>,
) -> Vec<GameRow> {
    let mut by_game: HashMap<u32, Vec<RecentIcon>> = HashMap::new();
    for (app_id, icon) in recent {
        by_game.entry(app_id).or_default().push(icon);
    }
    for icons in by_game.values_mut() {
        icons.sort_by_key(|icon| std::cmp::Reverse(icon.unlocked_at));
        icons.truncate(RECENT_ICONS);
    }
    let mut rows: Vec<GameRow> = overview
        .into_iter()
        .map(|g| GameRow {
            app_id: g.app_id,
            name: display_name(g.app_id, g.name),
            unlocked: g.unlocked,
            total: g.total,
            last_unlock_at: g.last_unlock_at,
            cover_url: cover_url(g.app_id),
            playtime_seconds: g.playtime_seconds,
            last_played: g.last_played,
            playing: playing.contains(&g.app_id),
            origin: g.origin.as_deref().map(origin_name),
            origin_id: g.origin.clone(),
            header_url: art_url(g.app_id, ArtKind::Header),
            platinum_at: g.platinum_at,
            recent: by_game.remove(&g.app_id).unwrap_or_default(),
        })
        .collect();
    rows.sort_by(|a, b| {
        b.playing
            .cmp(&a.playing)
            .then_with(|| activity(b).cmp(&activity(a)))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    rows
}

fn activity(row: &GameRow) -> Option<i64> {
    row.last_unlock_at.max(row.last_played)
}

pub fn order_achievements(mut rows: Vec<AchievementView>) -> Vec<AchievementView> {
    rows.sort_by(|a, b| {
        let pa = a.percent.unwrap_or(f64::INFINITY);
        let pb = b.percent.unwrap_or(f64::INFINITY);
        pa.total_cmp(&pb)
            .then_with(|| a.api_name.to_lowercase().cmp(&b.api_name.to_lowercase()))
    });
    rows
}

fn cached_icons(cache_dir: &std::path::Path, app_id: u32) -> HashSet<String> {
    std::fs::read_dir(cache_dir.join("icons").join(app_id.to_string()))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

fn icon_link(cached: &HashSet<String>, app_id: u32, file: Option<&str>) -> Option<String> {
    let file = file?;
    Some(if cached.contains(file) {
        format!("{PROTOCOL_ORIGIN}/icon/{app_id}/{file}")
    } else {
        format!("{COMMUNITY}/{app_id}/{file}")
    })
}

#[tauri::command]
pub async fn list_games(state: State<'_, AppState>) -> Result<Vec<GameRow>, String> {
    crate::games::refresh_steam_playtime(&state);
    let overview = state.store().games_overview().map_err(|e| e.to_string())?;
    let mut icons: HashMap<u32, HashSet<String>> = HashMap::new();
    let recent: Vec<(u32, RecentIcon)> = state
        .store()
        .recent_unlocks(RECENT_ICONS)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(app_id, v)| {
            let cached = icons
                .entry(app_id)
                .or_insert_with(|| cached_icons(&state.paths.cache_dir, app_id));
            Some((
                app_id,
                RecentIcon {
                    icon_url: icon_link(cached, app_id, v.icon.as_deref()),
                    display_name: v.display_name.unwrap_or_else(|| v.api_name.clone()),
                    description: v.description.unwrap_or_default(),
                    percent: v.percent,
                    unlocked_at: v.unlocked_at?,
                    playtime_seconds: v.playtime_seconds,
                    api_name: v.api_name,
                },
            ))
        })
        .collect();
    let playing: Vec<u32> = state.running().keys().copied().collect();
    let steam = steam_install_path();
    let mut rows = game_rows(overview, &playing, recent);
    for row in &mut rows {
        row.header_url = versioned_url(
            steam.as_deref(),
            &state.paths.cache_dir,
            row.app_id,
            ArtKind::Header,
        );
    }
    Ok(rows)
}

#[tauri::command]
pub async fn game_detail(state: State<'_, AppState>, app_id: u32) -> Result<GameDetail, String> {
    detail_for(&state, app_id)
}

#[tauri::command]
pub async fn overlay_game(state: State<'_, AppState>) -> Result<Option<GameDetail>, String> {
    state
        .current_game()
        .map(|app_id| detail_for(&state, app_id))
        .transpose()
}

#[tauri::command]
pub fn hotkey_error(state: State<'_, AppState>) -> Option<String> {
    state.hotkey_error().clone()
}

#[tauri::command]
pub fn ffmpeg_status(state: State<'_, AppState>) -> crate::video::FfmpegStatus {
    crate::video::status(&state)
}

#[tauri::command]
pub fn download_ffmpeg(app: AppHandle) {
    crate::video::download(app);
}

#[tauri::command]
pub fn cancel_ffmpeg(state: State<'_, AppState>) {
    crate::video::cancel_download(&state);
}

#[tauri::command]
pub fn update_status(state: State<'_, AppState>) -> crate::updater::UpdateStatus {
    crate::updater::status(&state)
}

#[tauri::command]
pub fn check_update(app: AppHandle) {
    std::thread::spawn(move || crate::updater::check(&app, false));
}

#[tauri::command]
pub fn install_update(app: AppHandle) {
    crate::updater::install_now(app);
}

#[tauri::command]
pub fn ui_locale(state: State<'_, AppState>) -> &'static str {
    state.ui_locale()
}

#[tauri::command]
pub fn supported_locales() -> Vec<crate::i18n::LocaleInfo> {
    crate::i18n::supported()
}

#[tauri::command]
pub fn toggle_overlay(app: AppHandle) {
    crate::overlay::toggle(&app);
}

fn detail_for(state: &AppState, app_id: u32) -> Result<GameDetail, String> {
    let store = state.store();
    let name = display_name(app_id, store.game_name(app_id).map_err(|e| e.to_string())?);
    let game = store.game_overview(app_id).map_err(|e| e.to_string())?;
    let playtime_seconds = game.as_ref().map_or(0, |g| g.playtime_seconds);
    let origin = game
        .as_ref()
        .and_then(|g| g.origin.as_deref())
        .map(origin_name);
    let platinum_at = game.and_then(|g| g.platinum_at);
    let views = order_achievements(store.game_achievements(app_id).map_err(|e| e.to_string())?);
    drop(store);
    let known = views.iter().filter(|v| v.display_name.is_some()).count();
    let cached = cached_icons(&state.paths.cache_dir, app_id);
    let achievements: Vec<AchievementRow> = views
        .into_iter()
        .map(|v| AchievementRow {
            icon_url: icon_link(
                &cached,
                app_id,
                v.icon.as_deref().or(v.icon_gray.as_deref()),
            ),
            icon_gray_url: icon_link(&cached, app_id, v.icon_gray.as_deref()),
            playtime_seconds: v.playtime_seconds,
            display_name: v.display_name.unwrap_or_else(|| v.api_name.clone()),
            description: v.description.unwrap_or_default(),
            api_name: v.api_name,
            hidden: v.hidden,
            percent: v.percent,
            unlocked_at: v.unlocked_at,
        })
        .collect();
    let unlocked = u32::try_from(
        achievements
            .iter()
            .filter(|a| a.unlocked_at.is_some())
            .count(),
    )
    .unwrap_or(u32::MAX);
    let steam = steam_install_path();
    let cache_dir = &state.paths.cache_dir;
    Ok(GameDetail {
        app_id,
        name,
        cover_url: cover_url(app_id),
        unlocked,
        total: if known == 0 {
            0
        } else {
            u32::try_from(achievements.len()).unwrap_or(u32::MAX)
        },
        playtime_seconds,
        playing: state.running().contains_key(&app_id),
        origin,
        header_url: versioned_url(steam.as_deref(), cache_dir, app_id, ArtKind::Header),
        hero_url: versioned_url(steam.as_deref(), cache_dir, app_id, ArtKind::Hero),
        logo_url: versioned_url(steam.as_deref(), cache_dir, app_id, ArtKind::Logo),
        logo_position: steam
            .as_deref()
            .and_then(|steam| read_logo_position(steam, app_id)),
        platinum_at,
        achievements,
    })
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings_snapshot()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    state.apply_settings(&app, settings)?;
    Ok(state.settings_snapshot())
}

#[tauri::command]
pub fn list_presets(state: State<'_, AppState>) -> Vec<PresetInfo> {
    presets::catalog(&state.paths.builtin_presets, &state.paths.user_presets)
}

#[tauri::command]
pub fn list_sounds(state: State<'_, AppState>) -> Vec<SoundPack> {
    sounds::catalog(&state.paths.builtin_sounds, &state.paths.user_sounds)
}

#[tauri::command]
pub fn preview_sound(state: State<'_, AppState>, id: String, event: SoundEvent) -> Option<u64> {
    let volume = state.settings_snapshot().sound_volume;
    let source = state.sound_source(&id, event);
    let duration = source.as_deref().and_then(sound::duration_ms);
    sound::play(volume, source);
    duration
}

#[tauri::command]
pub fn open_user_folder(state: State<'_, AppState>, kind: String) -> Result<(), String> {
    let dir = match kind.as_str() {
        "presets" => &state.paths.user_presets,
        "sounds" => &state.paths.user_sounds,
        "data" => &state.paths.data_dir,
        other => return Err(format!("unknown folder: {other}")),
    };
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    open_path(dir.display().to_string())
}

#[tauri::command]
pub fn preview_preset(app: AppHandle, id: String, kind: PopupKind) {
    app::send_test_popup(&app, Some(id), kind, true);
}

#[tauri::command]
pub fn test_notification(app: AppHandle) {
    app::send_test_popup(&app, None, PopupKind::Unlock, false);
}

#[tauri::command]
pub fn rescan(state: State<'_, AppState>) -> bool {
    state.with_watcher(|w| w.request_full_sync()).is_some()
}

#[tauri::command]
pub fn diagnostics(state: State<'_, AppState>) -> Result<Diagnostics, String> {
    let snapshot = state.with_watcher(|w| w.snapshot()).unwrap_or_default();
    let sources = builtin_sources()
        .iter()
        .map(|s| {
            let root = state.folders.resolve(s.base).map(|base| {
                s.root
                    .split('/')
                    .fold(base.to_path_buf(), |dir, segment| dir.join(segment))
            });
            SourceRow {
                id: s.id.into(),
                name: s.name.into(),
                exists: root.as_ref().is_some_and(|r| r.is_dir()),
                root: root.map(|r| r.display().to_string()),
            }
        })
        .collect();
    let files = state.store().files_report().map_err(|e| e.to_string())?;
    let external = state.store().external_games().map_err(|e| e.to_string())?;
    Ok(Diagnostics {
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: state.paths.data_dir.display().to_string(),
        language: state.language(),
        last_sync_at: snapshot.last_sync_at,
        sources,
        watched: snapshot
            .watched
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        unresolved: snapshot.unresolved,
        pending: snapshot.pending,
        external,
        files,
        log_tail: diag::log_tail(&state.paths.logs_dir, 200),
    })
}

#[tauri::command]
pub fn export_diagnostics(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let report = diagnostics(state.clone())?;
    let mut entries = vec![
        (
            "diagnostics.json".to_owned(),
            serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
        ),
        (
            "settings.json".to_owned(),
            serde_json::to_vec_pretty(&state.settings_snapshot()).map_err(|e| e.to_string())?,
        ),
    ];
    for file in diag::log_files(&state.paths.logs_dir) {
        if let (Some(name), Ok(bytes)) = (file.file_name(), std::fs::read(&file)) {
            entries.push((format!("logs/{}", name.to_string_lossy()), bytes));
        }
    }
    let dir = app
        .path()
        .download_dir()
        .unwrap_or_else(|_| state.paths.data_dir.clone());
    let stamp = aw_core::watcher::unix_now();
    let target = dir.join(format!("achievement-watcher-diagnostics-{stamp}.zip"));
    diag::write_zip(&target, &entries)?;
    Ok(target.display().to_string())
}

#[tauri::command]
pub fn export_game_report(
    app: AppHandle,
    state: State<'_, AppState>,
    app_id: u32,
) -> Result<String, String> {
    let found = state
        .with_watcher(|w| w.snapshot())
        .unwrap_or_default()
        .found;
    let stamp = aw_core::watcher::unix_now();
    let mut entries = report::build(
        &state.store(),
        &found,
        &WindowsRegistry,
        app_id,
        env!("CARGO_PKG_VERSION"),
        stamp,
    )?;
    if let Some(log) = diag::log_files(&state.paths.logs_dir).pop()
        && let (Some(name), Ok(bytes)) = (log.file_name(), std::fs::read(&log))
    {
        entries.push((format!("logs/{}", name.to_string_lossy()), bytes));
    }
    let dir = app
        .path()
        .download_dir()
        .unwrap_or_else(|_| state.paths.data_dir.clone());
    let target = dir.join(format!("achievement-watcher-report-{app_id}-{stamp}.zip"));
    diag::write_zip(&target, &entries)?;
    Ok(target.display().to_string())
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    let target = if path.is_file() {
        path.parent().map(PathBuf::from).unwrap_or(path)
    } else {
        path
    };
    if !target.is_dir() {
        return Err(format!("folder does not exist: {}", target.display()));
    }
    std::process::Command::new("explorer")
        .arg(&target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_come_from_the_cache_listing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("icons").join("7");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.jpg"), b"x").unwrap();
        let cached = cached_icons(tmp.path(), 7);
        assert_eq!(
            icon_link(&cached, 7, Some("a.jpg")).as_deref(),
            Some("http://aw.localhost/icon/7/a.jpg")
        );
        assert_eq!(
            icon_link(&cached, 7, Some("b.jpg")),
            Some(format!("{COMMUNITY}/7/b.jpg"))
        );
        assert_eq!(icon_link(&cached, 7, None), None);
        assert!(cached_icons(tmp.path(), 8).is_empty());
    }

    fn view(name: &str, percent: Option<f64>) -> AchievementView {
        AchievementView {
            api_name: name.into(),
            display_name: None,
            description: None,
            icon: None,
            icon_gray: None,
            hidden: false,
            percent,
            unlocked_at: None,
            playtime_seconds: None,
        }
    }

    #[test]
    fn game_rows_sort_playing_then_recent_activity() {
        let game = |app_id: u32,
                    name: Option<&str>,
                    last_unlock_at: Option<i64>,
                    last_played: Option<i64>| {
            GameOverview {
                app_id,
                name: name.map(Into::into),
                unlocked: 0,
                total: 0,
                last_unlock_at,
                playtime_seconds: 0,
                last_played,
                origin: None,
                platinum_at: None,
            }
        };
        let rows = game_rows(
            vec![
                game(1, Some("Zeta"), None, None),
                game(2, None, Some(10), None),
                game(3, Some("Alpha"), Some(50), None),
                game(4, Some("Beta"), None, Some(80)),
                game(5, Some("Running"), None, None),
            ],
            &[5],
            Vec::new(),
        );
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Running", "Beta", "Alpha", "App 2", "Zeta"]);
        assert!(rows[0].playing);
        assert_eq!(
            rows[2].cover_url,
            "https://cdn.akamai.steamstatic.com/steam/apps/3/header.jpg"
        );
    }

    fn overview(app_id: u32, origin: Option<&str>) -> GameOverview {
        GameOverview {
            app_id,
            name: Some(format!("Game {app_id}")),
            unlocked: 0,
            total: 0,
            last_unlock_at: None,
            playtime_seconds: 0,
            last_played: None,
            origin: origin.map(Into::into),
            platinum_at: None,
        }
    }

    fn recent(app_id: u32, api_name: &str, unlocked_at: i64) -> (u32, RecentIcon) {
        (
            app_id,
            RecentIcon {
                api_name: api_name.into(),
                display_name: api_name.into(),
                description: String::new(),
                icon_url: None,
                percent: None,
                unlocked_at,
                playtime_seconds: None,
            },
        )
    }

    #[test]
    fn game_rows_expose_origin_and_recent() {
        let rows = game_rows(
            vec![
                overview(7, Some("goldberg")),
                overview(8, Some("steam")),
                overview(9, Some("my_custom")),
            ],
            &[],
            (0..8)
                .map(|i| recent(7, &format!("A{i}"), 100 + i))
                .chain([recent(8, "S", 5)])
                .collect(),
        );
        let row = |id: u32| rows.iter().find(|r| r.app_id == id).unwrap();
        assert_eq!(row(7).origin.as_deref(), Some("Goldberg"));
        assert_eq!(row(8).origin.as_deref(), Some("Steam"));
        assert_eq!(row(9).origin.as_deref(), Some("my_custom"));
        assert_eq!(row(7).origin_id.as_deref(), Some("goldberg"));
        assert_eq!(row(8).origin_id.as_deref(), Some("steam"));
        let names: Vec<&str> = row(7).recent.iter().map(|r| r.api_name.as_str()).collect();
        assert_eq!(names, ["A7", "A6", "A5", "A4", "A3", "A2"]);
        assert_eq!(row(8).recent.len(), 1);
        assert!(row(9).recent.is_empty());
    }

    #[test]
    fn art_urls_use_protocol() {
        assert_eq!(
            crate::art::art_url(413150, crate::art::ArtKind::Hero),
            "http://aw.localhost/art/413150/hero"
        );
        let rows = game_rows(vec![overview(413150, None)], &[], Vec::new());
        assert_eq!(rows[0].header_url, "http://aw.localhost/art/413150/header");
        assert_eq!(rows[0].origin, None);
        assert_eq!(rows[0].origin_id, None);
    }

    #[test]
    fn rare_first_and_unknown_percent_last() {
        let ordered = order_achievements(vec![
            view("common", Some(80.0)),
            view("nodata", None),
            view("rare", Some(1.5)),
            view("Mid", Some(20.0)),
        ]);
        let names: Vec<&str> = ordered.iter().map(|v| v.api_name.as_str()).collect();
        assert_eq!(names, ["rare", "Mid", "common", "nodata"]);
    }

    #[test]
    fn origin_names_cover_registry_and_marker_sources() {
        assert_eq!(origin_name("lumaplay"), "LumaPlay");
        assert_eq!(origin_name("universelan_game"), "UniverseLAN");
        assert_eq!(origin_name("rune_uplay_documents"), "RUNE Ubisoft");
        assert_eq!(origin_name("nemirtingas_epic"), "Nemirtingas Epic");
    }
}
