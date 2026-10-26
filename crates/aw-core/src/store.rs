use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::Serialize;

use crate::parsers::LocalText;
use crate::platform::{ExternalId, GameImages, Platform, next_synthetic, synthetic_id};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
}

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SchemaRow {
    pub api_name: String,
    pub display_name: String,
    pub description: String,
    pub icon: Option<String>,
    pub icon_gray: Option<String>,
    pub hidden: bool,
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameOverview {
    pub app_id: u32,
    pub name: Option<String>,
    pub unlocked: u32,
    pub total: u32,
    pub last_unlock_at: Option<i64>,
    pub playtime_seconds: i64,
    pub last_played: Option<i64>,
    pub origin: Option<String>,
    pub platinum_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AchievementView {
    pub api_name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub icon_gray: Option<String>,
    pub hidden: bool,
    pub percent: Option<f64>,
    pub unlocked_at: Option<i64>,
    pub playtime_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileReport {
    pub path: String,
    pub source_id: String,
    pub app_id: u32,
    pub last_ok_at: Option<i64>,
    pub last_error: Option<String>,
    pub last_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExternalGame {
    pub platform: Platform,
    pub external_id: String,
    pub app_id: u32,
    pub title: Option<String>,
    pub images: GameImages,
    pub resolved_at: i64,
}

pub struct Store {
    conn: Connection,
}

const STEAM_SOURCE: &str = "steam";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS games (
    app_id INTEGER PRIMARY KEY,
    name TEXT,
    schema_lang TEXT,
    schema_fetched_at INTEGER
);
CREATE TABLE IF NOT EXISTS achievement_schema (
    app_id INTEGER NOT NULL,
    api_name TEXT NOT NULL COLLATE NOCASE,
    display_name TEXT NOT NULL,
    description TEXT NOT NULL,
    icon TEXT,
    icon_gray TEXT,
    hidden INTEGER NOT NULL,
    percent REAL,
    PRIMARY KEY (app_id, api_name)
);
CREATE TABLE IF NOT EXISTS unlocks (
    app_id INTEGER NOT NULL,
    api_name TEXT NOT NULL COLLATE NOCASE,
    unlocked_at INTEGER NOT NULL,
    detected_at INTEGER NOT NULL,
    source_id TEXT NOT NULL,
    PRIMARY KEY (app_id, api_name)
);
CREATE TABLE IF NOT EXISTS files (
    path TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    app_id INTEGER NOT NULL,
    content_hash TEXT,
    last_ok_at INTEGER,
    last_error TEXT,
    last_count INTEGER
);
CREATE TABLE IF NOT EXISTS progress (
    app_id INTEGER NOT NULL,
    api_name TEXT NOT NULL COLLATE NOCASE,
    current INTEGER NOT NULL,
    max INTEGER NOT NULL,
    PRIMARY KEY (app_id, api_name)
);
CREATE TABLE IF NOT EXISTS playtime (
    app_id INTEGER PRIMARY KEY,
    seconds INTEGER NOT NULL,
    last_played INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS steam_playtime (
    app_id INTEGER PRIMARY KEY,
    minutes INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS external_games (
    platform TEXT NOT NULL,
    external_id TEXT NOT NULL COLLATE NOCASE,
    app_id INTEGER NOT NULL,
    title TEXT,
    images TEXT,
    resolved_at INTEGER NOT NULL,
    PRIMARY KEY (platform, external_id)
);
";

const LOCAL_SCHEMA_LANG: &str = "local";

impl Store {
    pub fn open(path: &Path) -> StoreResult<Store> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Store::init(conn)
    }

    pub fn in_memory() -> StoreResult<Store> {
        Store::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> StoreResult<Store> {
        conn.execute_batch(SCHEMA)?;
        let store = Store { conn };
        store.ensure_column("unlocks", "playtime_seconds", "INTEGER")?;
        Ok(store)
    }

    fn ensure_column(&self, table: &str, column: &str, decl: &str) -> StoreResult<()> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<_, _>>()?;
        if !names.iter().any(|n| n.eq_ignore_ascii_case(column)) {
            self.conn
                .execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"))?;
        }
        Ok(())
    }

    pub fn set_unlock_playtime(
        &self,
        app_id: u32,
        api_name: &str,
        seconds: i64,
    ) -> StoreResult<()> {
        self.conn.execute(
            "UPDATE unlocks SET playtime_seconds = ?3 WHERE app_id = ?1 AND api_name = ?2",
            params![app_id, api_name, seconds.max(0)],
        )?;
        Ok(())
    }

    pub fn steam_minutes(&self, app_id: u32) -> StoreResult<Option<u64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT minutes FROM steam_playtime WHERE app_id = ?1",
                params![app_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(|m| u64::try_from(m).unwrap_or(0)))
    }

    pub fn aw_playtime_seconds(&self, app_id: u32) -> StoreResult<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT seconds FROM playtime WHERE app_id = ?1",
                params![app_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    pub fn replace_steam_playtime(
        &self,
        minutes: &std::collections::HashMap<u32, u64>,
    ) -> StoreResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM steam_playtime", [])?;
        {
            let mut insert =
                tx.prepare("INSERT INTO steam_playtime (app_id, minutes) VALUES (?1, ?2)")?;
            for (app_id, value) in minutes {
                insert.execute(params![app_id, i64::try_from(*value).unwrap_or(i64::MAX)])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn begin(&self) -> StoreResult<Transaction<'_>> {
        Ok(self.conn.unchecked_transaction()?)
    }

    pub fn is_unlocked(&self, app_id: u32, api_name: &str) -> StoreResult<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM unlocks WHERE app_id = ?1 AND api_name = ?2",
                params![app_id, api_name],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn record_unlock(
        &self,
        app_id: u32,
        api_name: &str,
        unlocked_at: i64,
        detected_at: i64,
        source_id: &str,
    ) -> StoreResult<bool> {
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO unlocks (app_id, api_name, unlocked_at, detected_at, source_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![app_id, api_name, unlocked_at, detected_at, source_id],
        )?;
        if inserted == 0 {
            self.conn.execute(
                "UPDATE unlocks SET unlocked_at = ?3 WHERE app_id = ?1 AND api_name = ?2 AND unlocked_at > ?3",
                params![app_id, api_name, unlocked_at],
            )?;
        }
        Ok(inserted > 0)
    }

    pub fn unlock_time(&self, app_id: u32, api_name: &str) -> StoreResult<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT unlocked_at FROM unlocks WHERE app_id = ?1 AND api_name = ?2",
                params![app_id, api_name],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn file_hash(&self, path: &str) -> StoreResult<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT content_hash FROM files WHERE path = ?1",
                params![path],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    pub fn mark_file_ok(
        &self,
        path: &str,
        source_id: &str,
        app_id: u32,
        hash: &str,
        count: u32,
        now: i64,
    ) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO files (path, source_id, app_id, content_hash, last_ok_at, last_error, last_count)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)
             ON CONFLICT(path) DO UPDATE SET source_id = ?2, app_id = ?3, content_hash = ?4,
                 last_ok_at = ?5, last_error = NULL, last_count = ?6",
            params![path, source_id, app_id, hash, now, count],
        )?;
        Ok(())
    }

    pub fn mark_file_error(
        &self,
        path: &str,
        source_id: &str,
        app_id: u32,
        error: &str,
    ) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO files (path, source_id, app_id, last_error) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET source_id = ?2, app_id = ?3, last_error = ?4",
            params![path, source_id, app_id, error],
        )?;
        Ok(())
    }

    pub fn meta_get(&self, key: &str) -> StoreResult<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn meta_set(&self, key: &str, value: &str) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn ensure_game(&self, app_id: u32) -> StoreResult<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO games (app_id) VALUES (?1)",
            params![app_id],
        )?;
        Ok(())
    }

    pub fn game_name(&self, app_id: u32) -> StoreResult<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT name FROM games WHERE app_id = ?1",
                params![app_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    pub fn schema_info(&self, app_id: u32) -> StoreResult<Option<(String, i64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT schema_lang, schema_fetched_at FROM games WHERE app_id = ?1 AND schema_fetched_at IS NOT NULL",
                params![app_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    pub fn replace_schema(
        &self,
        app_id: u32,
        lang: &str,
        name: Option<&str>,
        rows: &[SchemaRow],
        now: i64,
    ) -> StoreResult<()> {
        self.atomic(|conn| {
            conn.execute(
                "INSERT INTO games (app_id, name, schema_lang, schema_fetched_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(app_id) DO UPDATE SET name = COALESCE(?2, name), schema_lang = ?3, schema_fetched_at = ?4",
                params![app_id, name, lang, now],
            )?;
            conn.execute(
                "DELETE FROM achievement_schema WHERE app_id = ?1",
                params![app_id],
            )?;
            let mut insert = conn.prepare(
                "INSERT OR REPLACE INTO achievement_schema
                 (app_id, api_name, display_name, description, icon, icon_gray, hidden, percent)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for row in rows {
                insert.execute(params![
                    app_id,
                    row.api_name,
                    row.display_name,
                    row.description,
                    row.icon,
                    row.icon_gray,
                    row.hidden,
                    row.percent
                ])?;
            }
            Ok(())
        })
    }

    fn atomic<T>(&self, work: impl FnOnce(&Connection) -> StoreResult<T>) -> StoreResult<T> {
        if !self.conn.is_autocommit() {
            return work(&self.conn);
        }
        let tx = self.begin()?;
        let value = work(&tx)?;
        tx.commit()?;
        Ok(value)
    }

    pub fn external_app_id(&self, key: &ExternalId) -> StoreResult<Option<u32>> {
        Ok(self
            .conn
            .query_row(
                "SELECT app_id FROM external_games WHERE platform = ?1 AND external_id = ?2",
                params![key.platform.as_str(), key.id],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn save_external_game(
        &self,
        key: &ExternalId,
        steam_app_id: Option<u32>,
        title: Option<&str>,
        images: &GameImages,
        now: i64,
    ) -> StoreResult<u32> {
        let app_id = match steam_app_id {
            Some(app_id) => app_id,
            None => self.free_synthetic(key)?,
        };
        let images = serde_json::to_string(images).unwrap_or_default();
        self.atomic(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO external_games (platform, external_id, app_id, title, images, resolved_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![key.platform.as_str(), key.id, app_id, title, images, now],
            )?;
            conn.execute(
                "INSERT INTO games (app_id, name) VALUES (?1, ?2)
                 ON CONFLICT(app_id) DO UPDATE SET name = COALESCE(games.name, ?2)",
                params![app_id, title],
            )?;
            Ok(app_id)
        })
    }

    fn free_synthetic(&self, key: &ExternalId) -> StoreResult<u32> {
        let mut app_id = synthetic_id(key);
        loop {
            let owner: Option<(String, String)> = self
                .conn
                .query_row(
                    "SELECT platform, external_id FROM external_games WHERE app_id = ?1",
                    params![app_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            match owner {
                Some((platform, id))
                    if platform != key.platform.as_str() || !id.eq_ignore_ascii_case(&key.id) =>
                {
                    app_id = next_synthetic(app_id);
                }
                _ => return Ok(app_id),
            }
        }
    }

    pub fn external_game(&self, app_id: u32) -> StoreResult<Option<ExternalGame>> {
        Ok(self
            .conn
            .query_row(
                "SELECT platform, external_id, app_id, title, images, resolved_at
                 FROM external_games WHERE app_id = ?1 ORDER BY platform, external_id LIMIT 1",
                params![app_id],
                external_row,
            )
            .optional()?
            .flatten())
    }

    pub fn external_games(&self) -> StoreResult<Vec<ExternalGame>> {
        let mut stmt = self.conn.prepare(
            "SELECT platform, external_id, app_id, title, images, resolved_at
             FROM external_games ORDER BY platform, external_id",
        )?;
        let rows = stmt.query_map([], external_row)?;
        Ok(rows
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect())
    }

    pub fn replace_local_schema(
        &self,
        app_id: u32,
        texts: &[LocalText],
        now: i64,
    ) -> StoreResult<bool> {
        if self
            .schema_info(app_id)?
            .is_some_and(|(lang, _)| lang != LOCAL_SCHEMA_LANG)
        {
            return Ok(false);
        }
        let mut current: Vec<(String, String, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT api_name, display_name, description FROM achievement_schema WHERE app_id = ?1",
            )?;
            let rows = stmt.query_map(params![app_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            rows.collect::<Result<_, _>>()?
        };
        let mut wanted: Vec<(String, String, String)> = texts
            .iter()
            .map(|t| {
                (
                    t.api_name.clone(),
                    t.display_name.clone(),
                    t.description.clone(),
                )
            })
            .collect();
        current.sort();
        wanted.sort();
        wanted.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
        if current == wanted && self.schema_info(app_id)?.is_some() {
            return Ok(false);
        }
        let rows: Vec<SchemaRow> = wanted
            .into_iter()
            .map(|(api_name, display_name, description)| SchemaRow {
                api_name,
                display_name,
                description,
                icon: None,
                icon_gray: None,
                hidden: false,
                percent: None,
            })
            .collect();
        self.replace_schema(app_id, LOCAL_SCHEMA_LANG, None, &rows, now)?;
        Ok(true)
    }

    pub fn update_progress(
        &self,
        app_id: u32,
        api_name: &str,
        current: u32,
        max: u32,
    ) -> StoreResult<Option<u32>> {
        let previous: Option<u32> = self
            .conn
            .query_row(
                "SELECT current FROM progress WHERE app_id = ?1 AND api_name = ?2",
                params![app_id, api_name],
                |row| row.get(0),
            )
            .optional()?;
        self.conn.execute(
            "INSERT INTO progress (app_id, api_name, current, max) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(app_id, api_name) DO UPDATE SET current = ?3, max = ?4",
            params![app_id, api_name, current, max],
        )?;
        Ok(previous)
    }

    pub fn add_playtime(&self, app_id: u32, seconds: i64, now: i64) -> StoreResult<()> {
        self.ensure_game(app_id)?;
        self.conn.execute(
            "INSERT INTO playtime (app_id, seconds, last_played) VALUES (?1, ?2, ?3)
             ON CONFLICT(app_id) DO UPDATE SET seconds = seconds + ?2, last_played = ?3",
            params![app_id, seconds.max(0), now],
        )?;
        Ok(())
    }

    pub fn schema_icons(&self, app_id: u32) -> StoreResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT icon FROM achievement_schema WHERE app_id = ?1 AND icon IS NOT NULL
             UNION SELECT icon_gray FROM achievement_schema WHERE app_id = ?1 AND icon_gray IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![app_id], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn schema_names(&self, app_id: u32) -> StoreResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT api_name FROM achievement_schema WHERE app_id = ?1")?;
        let rows = stmt.query_map(params![app_id], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn schema_row(&self, app_id: u32, api_name: &str) -> StoreResult<Option<SchemaRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT api_name, display_name, description, icon, icon_gray, hidden, percent
                 FROM achievement_schema WHERE app_id = ?1 AND api_name = ?2",
                params![app_id, api_name],
                |row| {
                    Ok(SchemaRow {
                        api_name: row.get(0)?,
                        display_name: row.get(1)?,
                        description: row.get(2)?,
                        icon: row.get(3)?,
                        icon_gray: row.get(4)?,
                        hidden: row.get(5)?,
                        percent: row.get(6)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn games_overview(&self) -> StoreResult<Vec<GameOverview>> {
        self.overview_rows(None)
    }

    pub fn game_overview(&self, app_id: u32) -> StoreResult<Option<GameOverview>> {
        Ok(self.overview_rows(Some(app_id))?.into_iter().next())
    }

    fn overview_rows(&self, only: Option<u32>) -> StoreResult<Vec<GameOverview>> {
        let mut stmt = self.conn.prepare(
            "WITH counted AS (
                 SELECT u.app_id, u.unlocked_at FROM unlocks u
                 WHERE NOT EXISTS (SELECT 1 FROM achievement_schema s0 WHERE s0.app_id = u.app_id)
                    OR EXISTS (SELECT 1 FROM achievement_schema s
                               WHERE s.app_id = u.app_id AND s.api_name = u.api_name))
             SELECT g.app_id, g.name,
                    (SELECT COUNT(*) FROM counted c WHERE c.app_id = g.app_id),
                    (SELECT COUNT(*) FROM achievement_schema s WHERE s.app_id = g.app_id),
                    (SELECT MAX(unlocked_at) FROM unlocks u WHERE u.app_id = g.app_id),
                    COALESCE((SELECT minutes * 60 FROM steam_playtime sp WHERE sp.app_id = g.app_id),
                             (SELECT seconds FROM playtime p WHERE p.app_id = g.app_id), 0),
                    (SELECT last_played FROM playtime p WHERE p.app_id = g.app_id),
                    (SELECT source_id FROM unlocks u WHERE u.app_id = g.app_id
                     ORDER BY unlocked_at DESC, detected_at DESC LIMIT 1),
                    (SELECT CASE WHEN COUNT(DISTINCT source_id) = 1 THEN MAX(source_id) END
                     FROM files f WHERE f.app_id = g.app_id AND f.source_id <> 'steam'),
                    (SELECT MAX(unlocked_at) FROM counted c WHERE c.app_id = g.app_id),
                    NOT EXISTS (SELECT 1 FROM files f WHERE f.app_id = g.app_id AND f.source_id <> 'steam')
                    AND (EXISTS (SELECT 1 FROM files f WHERE f.app_id = g.app_id AND f.source_id = 'steam')
                         OR EXISTS (SELECT 1 FROM steam_playtime sp WHERE sp.app_id = g.app_id))
             FROM games g
             WHERE ?1 IS NULL OR g.app_id = ?1",
        )?;
        let rows = stmt.query_map(params![only], |row| {
            let unlocked: u32 = row.get(2)?;
            let total: u32 = row.get(3)?;
            let latest: Option<String> = row.get(7)?;
            let single_file: Option<String> = row.get(8)?;
            let counted_last: Option<i64> = row.get(9)?;
            let steam_only: bool = row.get(10)?;
            Ok(GameOverview {
                app_id: row.get(0)?,
                name: row.get(1)?,
                unlocked,
                total: if total == 0 { 0 } else { total.max(unlocked) },
                last_unlock_at: row.get(4)?,
                playtime_seconds: row.get(5)?,
                last_played: row.get(6)?,
                origin: latest
                    .or(single_file)
                    .or_else(|| steam_only.then(|| STEAM_SOURCE.to_owned())),
                platinum_at: (total > 0 && unlocked >= total)
                    .then_some(counted_last)
                    .flatten(),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn game_achievements(&self, app_id: u32) -> StoreResult<Vec<AchievementView>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.api_name, s.display_name, s.description, s.icon, s.icon_gray, s.hidden, s.percent, u.unlocked_at, u.playtime_seconds
             FROM achievement_schema s
             LEFT JOIN unlocks u ON u.app_id = s.app_id AND u.api_name = s.api_name
             WHERE s.app_id = ?1
             UNION ALL
             SELECT u.api_name, NULL, NULL, NULL, NULL, 0, NULL, u.unlocked_at, u.playtime_seconds
             FROM unlocks u
             WHERE u.app_id = ?1 AND NOT EXISTS (
                 SELECT 1 FROM achievement_schema s WHERE s.app_id = u.app_id AND s.api_name = u.api_name)",
        )?;
        let rows = stmt.query_map(params![app_id], |row| {
            Ok(AchievementView {
                api_name: row.get(0)?,
                display_name: row.get(1)?,
                description: row.get(2)?,
                icon: row.get(3)?,
                icon_gray: row.get(4)?,
                hidden: row.get(5)?,
                percent: row.get(6)?,
                unlocked_at: row.get(7)?,
                playtime_seconds: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn recent_unlocks(&self, per_game: usize) -> StoreResult<Vec<(u32, AchievementView)>> {
        let mut stmt = self.conn.prepare(
            "SELECT app_id, api_name, display_name, description, icon, icon_gray, hidden, percent, unlocked_at, playtime_seconds
             FROM (
                 SELECT u.app_id, u.api_name, s.display_name, s.description, s.icon, s.icon_gray,
                        COALESCE(s.hidden, 0) AS hidden, s.percent, u.unlocked_at, u.playtime_seconds,
                        ROW_NUMBER() OVER (PARTITION BY u.app_id ORDER BY u.unlocked_at DESC, u.api_name) AS n
                 FROM unlocks u
                 LEFT JOIN achievement_schema s ON s.app_id = u.app_id AND s.api_name = u.api_name)
             WHERE n <= ?1
             ORDER BY app_id, unlocked_at DESC",
        )?;
        let limit = i64::try_from(per_game).unwrap_or(i64::MAX);
        let rows = stmt.query_map(params![limit], |row| {
            Ok((
                row.get(0)?,
                AchievementView {
                    api_name: row.get(1)?,
                    display_name: row.get(2)?,
                    description: row.get(3)?,
                    icon: row.get(4)?,
                    icon_gray: row.get(5)?,
                    hidden: row.get(6)?,
                    percent: row.get(7)?,
                    unlocked_at: row.get(8)?,
                    playtime_seconds: row.get(9)?,
                },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn files_report(&self) -> StoreResult<Vec<FileReport>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, source_id, app_id, last_ok_at, last_error, last_count FROM files ORDER BY app_id, path",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(FileReport {
                path: row.get(0)?,
                source_id: row.get(1)?,
                app_id: row.get(2)?,
                last_ok_at: row.get(3)?,
                last_error: row.get(4)?,
                last_count: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

fn external_row(row: &Row<'_>) -> rusqlite::Result<Option<ExternalGame>> {
    let platform: String = row.get(0)?;
    let external_id: String = row.get(1)?;
    let app_id: u32 = row.get(2)?;
    let title: Option<String> = row.get(3)?;
    let images: Option<String> = row.get(4)?;
    let resolved_at: i64 = row.get(5)?;
    Ok(Platform::parse(&platform).map(|platform| ExternalGame {
        platform,
        external_id,
        app_id,
        title,
        images: images
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default(),
        resolved_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, percent: f64) -> SchemaRow {
        SchemaRow {
            api_name: name.into(),
            display_name: format!("{name} display"),
            description: String::new(),
            icon: Some(format!("{name}.jpg")),
            icon_gray: None,
            hidden: false,
            percent: Some(percent),
        }
    }

    #[test]
    fn record_unlock_is_new_once_and_keeps_earliest_time() {
        let s = Store::in_memory().unwrap();
        assert!(s.record_unlock(7, "A", 200, 300, "onlinefix").unwrap());
        assert!(!s.record_unlock(7, "A", 100, 400, "goldberg").unwrap());
        assert!(!s.record_unlock(7, "A", 150, 500, "goldberg").unwrap());
        assert_eq!(s.unlock_time(7, "A").unwrap(), Some(100));
    }

    #[test]
    fn api_name_is_case_insensitive() {
        let s = Store::in_memory().unwrap();
        s.record_unlock(7, "Ach_One", 100, 100, "onlinefix")
            .unwrap();
        assert!(s.is_unlocked(7, "ACH_ONE").unwrap());
        assert!(!s.record_unlock(7, "ach_one", 100, 100, "gse").unwrap());
        assert!(!s.is_unlocked(8, "ACH_ONE").unwrap());
    }

    #[test]
    fn file_hash_roundtrip_and_error_keeps_old_hash() {
        let s = Store::in_memory().unwrap();
        assert_eq!(s.file_hash("p").unwrap(), None);
        s.mark_file_ok("p", "onlinefix", 7, "h1", 3, 10).unwrap();
        assert_eq!(s.file_hash("p").unwrap(), Some("h1".into()));
        s.mark_file_error("p", "onlinefix", 7, "empty file")
            .unwrap();
        assert_eq!(s.file_hash("p").unwrap(), Some("h1".into()));
        let report = s.files_report().unwrap();
        assert_eq!(report[0].last_error.as_deref(), Some("empty file"));
        assert_eq!(report[0].last_ok_at, Some(10));
        s.mark_file_error("q", "gse", 8, "boom").unwrap();
        assert_eq!(s.file_hash("q").unwrap(), None);
    }

    #[test]
    fn schema_replace_is_atomic_and_overview_counts() {
        let s = Store::in_memory().unwrap();
        s.replace_schema(
            7,
            "english",
            Some("Game"),
            &[row("A", 50.0), row("B", 5.0)],
            1,
        )
        .unwrap();
        s.replace_schema(7, "english", None, &[row("A", 40.0), row("C", 1.0)], 2)
            .unwrap();
        s.record_unlock(7, "a", 100, 100, "onlinefix").unwrap();
        s.record_unlock(7, "Z", 90, 100, "onlinefix").unwrap();
        assert_eq!(s.schema_info(7).unwrap(), Some(("english".into(), 2)));
        assert_eq!(s.game_name(7).unwrap(), Some("Game".into()));
        let overview = s.games_overview().unwrap();
        assert_eq!(
            overview,
            vec![GameOverview {
                app_id: 7,
                name: Some("Game".into()),
                unlocked: 1,
                total: 2,
                last_unlock_at: Some(100),
                playtime_seconds: 0,
                last_played: None,
                origin: Some("onlinefix".into()),
                platinum_at: None,
            }]
        );
        let mut names: Vec<(String, Option<i64>)> = s
            .game_achievements(7)
            .unwrap()
            .into_iter()
            .map(|a| (a.api_name, a.unlocked_at))
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                ("A".into(), Some(100)),
                ("C".into(), None),
                ("Z".into(), Some(90))
            ]
        );
        assert_eq!(
            s.schema_row(7, "c").unwrap().unwrap().display_name,
            "C display"
        );
    }

    #[test]
    fn playtime_accumulates_sessions() {
        let s = Store::in_memory().unwrap();
        s.add_playtime(9, 120, 1000).unwrap();
        s.add_playtime(9, 60, 2000).unwrap();
        let g = s.games_overview().unwrap();
        assert_eq!(
            (g[0].app_id, g[0].playtime_seconds, g[0].last_played),
            (9, 180, Some(2000))
        );
    }

    #[test]
    fn total_is_unknown_without_schema() {
        let s = Store::in_memory().unwrap();
        s.record_unlock(3, "A", 1, 1, "steam").unwrap();
        s.ensure_game(3).unwrap();
        let g = s.games_overview().unwrap();
        assert_eq!((g[0].unlocked, g[0].total), (1, 0));
    }

    fn schema(s: &Store, app_id: u32, count: usize) {
        let rows: Vec<SchemaRow> = (0..count)
            .map(|i| row(&format!("ACH{i:02}"), 50.0))
            .collect();
        s.replace_schema(app_id, "english", Some("Game"), &rows, 1)
            .unwrap();
    }

    #[test]
    fn recent_unlocks_keeps_latest_per_game() {
        let s = Store::in_memory().unwrap();
        schema(&s, 1, 10);
        for i in 0..8 {
            s.record_unlock(1, &format!("ACH{i:02}"), 100 + i64::from(i), 1, "steam")
                .unwrap();
        }
        s.record_unlock(2, "X", 50, 1, "goldberg").unwrap();
        let recent = s.recent_unlocks(6).unwrap();
        let first: Vec<(&str, Option<i64>)> = recent
            .iter()
            .filter(|(id, _)| *id == 1)
            .map(|(_, v)| (v.api_name.as_str(), v.unlocked_at))
            .collect();
        assert_eq!(first.len(), 6);
        assert_eq!(first[0], ("ACH07", Some(107)));
        assert_eq!(first[5], ("ACH02", Some(102)));
        assert!(
            recent
                .iter()
                .any(|(id, v)| *id == 2 && v.display_name.is_none())
        );
    }

    #[test]
    fn overview_ignores_unlocks_outside_schema() {
        let s = Store::in_memory().unwrap();
        schema(&s, 1245620, 42);
        s.record_unlock(1245620, "ACH00", 10, 10, "goldberg")
            .unwrap();
        s.record_unlock(1245620, "ROUNDTABLE_HOLD", 20, 20, "goldberg")
            .unwrap();
        let g = &s.games_overview().unwrap()[0];
        assert_eq!((g.unlocked, g.total), (1, 42));
    }

    #[test]
    fn origin_is_source_of_latest_unlock() {
        let s = Store::in_memory().unwrap();
        schema(&s, 5, 3);
        s.record_unlock(5, "ACH00", 10, 10, "steam").unwrap();
        s.record_unlock(5, "ACH01", 20, 20, "goldberg").unwrap();
        assert_eq!(
            s.games_overview().unwrap()[0].origin.as_deref(),
            Some("goldberg")
        );
    }

    #[test]
    fn origin_is_single_non_steam_file_when_no_unlocks() {
        let s = Store::in_memory().unwrap();
        schema(&s, 6, 3);
        schema(&s, 7, 3);
        s.mark_file_ok("a", "codex_public", 6, "h", 0, 1).unwrap();
        s.mark_file_ok("b", "steam", 6, "h", 0, 1).unwrap();
        s.mark_file_ok("c", "codex_public", 7, "h", 0, 1).unwrap();
        s.mark_file_ok("d", "rune", 7, "h", 0, 1).unwrap();
        let mut g = s.games_overview().unwrap();
        g.sort_by_key(|o| o.app_id);
        assert_eq!(g[0].origin.as_deref(), Some("codex_public"));
        assert_eq!(g[1].origin, None);
    }

    #[test]
    fn one_game_overview_matches_the_full_list() {
        let s = Store::in_memory().unwrap();
        schema(&s, 5, 3);
        schema(&s, 6, 2);
        s.record_unlock(5, "ACH00", 10, 10, "goldberg").unwrap();
        let all = s.games_overview().unwrap();
        let five = s.game_overview(5).unwrap().unwrap();
        assert_eq!(Some(&five), all.iter().find(|g| g.app_id == 5));
        assert!(s.game_overview(99).unwrap().is_none());
    }

    #[test]
    fn games_known_only_to_steam_count_as_steam() {
        let s = Store::in_memory().unwrap();
        for app_id in 8..=11 {
            schema(&s, app_id, 3);
        }
        s.mark_file_ok("s8", "steam", 8, "h", 0, 1).unwrap();
        s.replace_steam_playtime(&std::collections::HashMap::from([(9, 60)]))
            .unwrap();
        s.mark_file_ok("c10", "codex_public", 10, "h", 0, 1)
            .unwrap();
        s.mark_file_ok("s10", "steam", 10, "h", 0, 1).unwrap();
        s.mark_file_ok("a11", "codex_public", 11, "h", 0, 1)
            .unwrap();
        s.mark_file_ok("b11", "rune", 11, "h", 0, 1).unwrap();
        s.mark_file_ok("s11", "steam", 11, "h", 0, 1).unwrap();
        let mut g = s.games_overview().unwrap();
        g.sort_by_key(|o| o.app_id);
        let origins: Vec<Option<&str>> = g.iter().map(|o| o.origin.as_deref()).collect();
        assert_eq!(
            origins,
            [Some("steam"), Some("steam"), Some("codex_public"), None]
        );
    }

    #[test]
    fn steam_playtime_wins_over_aw_counter() {
        let s = Store::in_memory().unwrap();
        s.add_playtime(413150, 100, 50).unwrap();
        s.replace_steam_playtime(&std::collections::HashMap::from([(413150, 470)]))
            .unwrap();
        assert_eq!(s.games_overview().unwrap()[0].playtime_seconds, 28200);
        s.replace_steam_playtime(&std::collections::HashMap::new())
            .unwrap();
        assert_eq!(s.games_overview().unwrap()[0].playtime_seconds, 100);
    }

    #[test]
    fn steam_minutes_and_aw_seconds_per_app() {
        let s = Store::in_memory().unwrap();
        s.add_playtime(1, 90, 10).unwrap();
        s.replace_steam_playtime(&std::collections::HashMap::from([(2, 30)]))
            .unwrap();
        assert_eq!(s.steam_minutes(2).unwrap(), Some(30));
        assert_eq!(s.steam_minutes(1).unwrap(), None);
        assert_eq!(s.aw_playtime_seconds(1).unwrap(), 90);
        assert_eq!(s.aw_playtime_seconds(2).unwrap(), 0);
    }

    #[test]
    fn unlock_playtime_round_trips() {
        let s = Store::in_memory().unwrap();
        schema(&s, 1, 2);
        s.record_unlock(1, "ACH00", 10, 10, "steam").unwrap();
        s.set_unlock_playtime(1, "ach00", 3600).unwrap();
        let rows = s.game_achievements(1).unwrap();
        let a = rows.iter().find(|r| r.api_name == "ACH00").unwrap();
        let b = rows.iter().find(|r| r.api_name == "ACH01").unwrap();
        assert_eq!((a.playtime_seconds, b.playtime_seconds), (Some(3600), None));
    }

    #[test]
    fn migration_adds_playtime_column_to_old_db() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("old.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE unlocks (app_id INTEGER NOT NULL, api_name TEXT NOT NULL COLLATE NOCASE,
                 unlocked_at INTEGER NOT NULL, detected_at INTEGER NOT NULL, source_id TEXT NOT NULL,
                 PRIMARY KEY (app_id, api_name));
                 INSERT INTO unlocks VALUES (1, 'A', 1, 1, 'steam');",
            )
            .unwrap();
        }
        let s = Store::open(&path).unwrap();
        s.set_unlock_playtime(1, "A", 60).unwrap();
        assert!(s.is_unlocked(1, "A").unwrap());
        drop(s);
        assert!(Store::open(&path).is_ok());
    }

    #[test]
    fn platinum_at_is_latest_unlock_when_complete() {
        let s = Store::in_memory().unwrap();
        schema(&s, 1583320, 2);
        s.record_unlock(1583320, "ACH00", 10, 10, "steam").unwrap();
        assert_eq!(s.games_overview().unwrap()[0].platinum_at, None);
        s.record_unlock(1583320, "ACH01", 30, 30, "steam").unwrap();
        assert_eq!(s.games_overview().unwrap()[0].platinum_at, Some(30));
    }

    #[test]
    fn meta_roundtrip() {
        let s = Store::in_memory().unwrap();
        assert_eq!(s.meta_get("k").unwrap(), None);
        s.meta_set("k", "1").unwrap();
        s.meta_set("k", "2").unwrap();
        assert_eq!(s.meta_get("k").unwrap(), Some("2".into()));
    }

    #[test]
    fn open_creates_file_database() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data.sqlite");
        {
            let s = Store::open(&path).unwrap();
            s.record_unlock(1, "A", 1, 1, "x").unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert!(s.is_unlocked(1, "A").unwrap());
    }

    fn key(platform: Platform, id: &str) -> ExternalId {
        ExternalId {
            platform,
            id: id.into(),
        }
    }

    fn text(api: &str, name: &str) -> LocalText {
        LocalText {
            api_name: api.into(),
            display_name: name.into(),
            description: format!("{name} description"),
        }
    }

    #[test]
    fn unknown_external_game_has_no_app_id() {
        let s = Store::in_memory().unwrap();
        assert_eq!(s.external_app_id(&key(Platform::Epic, "X")).unwrap(), None);
    }

    #[test]
    fn save_external_with_steam_uses_steam_id() {
        let s = Store::in_memory().unwrap();
        let images = GameImages::default();
        let id = s
            .save_external_game(
                &key(Platform::Ubisoft, "4"),
                Some(33230),
                Some("AC II"),
                &images,
                10,
            )
            .unwrap();
        assert_eq!(id, 33230);
        assert_eq!(
            s.external_app_id(&key(Platform::Ubisoft, "4")).unwrap(),
            Some(33230)
        );
        let game = s.external_game(33230).unwrap().unwrap();
        assert_eq!(game.platform, Platform::Ubisoft);
        assert_eq!(game.external_id, "4");
        assert_eq!(game.resolved_at, 10);
        assert_eq!(s.game_name(33230).unwrap(), Some("AC II".into()));
    }

    #[test]
    fn save_external_without_steam_uses_synthetic_and_names_game() {
        let s = Store::in_memory().unwrap();
        let images = GameImages {
            header: Some("https://h".into()),
            ..GameImages::default()
        };
        let k = key(Platform::Epic, "Carnation");
        let id = s
            .save_external_game(&k, None, Some("Siege"), &images, 5)
            .unwrap();
        assert_eq!(id, synthetic_id(&k));
        assert_eq!(s.game_name(id).unwrap(), Some("Siege".into()));
        assert_eq!(s.external_game(id).unwrap().unwrap().images, images);
        assert_eq!(
            s.external_app_id(&key(Platform::Epic, "carnation"))
                .unwrap(),
            Some(id)
        );
        assert_eq!(s.external_games().unwrap().len(), 1);
    }

    #[test]
    fn external_title_does_not_replace_existing_name() {
        let s = Store::in_memory().unwrap();
        s.replace_schema(20900, "english", Some("The Witcher"), &[], 1)
            .unwrap();
        s.save_external_game(
            &key(Platform::Gog, "1207658924"),
            Some(20900),
            Some("Witcher EE"),
            &GameImages::default(),
            2,
        )
        .unwrap();
        assert_eq!(s.game_name(20900).unwrap(), Some("The Witcher".into()));
    }

    #[test]
    fn synthetic_collision_moves_to_next_free() {
        let s = Store::in_memory().unwrap();
        let wanted = key(Platform::Gog, "A");
        let taken = synthetic_id(&wanted);
        s.conn
            .execute(
                "INSERT INTO external_games (platform, external_id, app_id, resolved_at) VALUES ('epic', 'other', ?1, 1)",
                params![taken],
            )
            .unwrap();
        let id = s
            .save_external_game(&wanted, None, None, &GameImages::default(), 2)
            .unwrap();
        assert_eq!(id, taken + 1);
    }

    #[test]
    fn local_schema_is_written_once_and_never_over_steam() {
        let s = Store::in_memory().unwrap();
        let texts = [text("A", "Alpha"), text("B", "Beta")];
        assert!(s.replace_local_schema(0x8000_0001, &texts, 1).unwrap());
        assert!(!s.replace_local_schema(0x8000_0001, &texts, 2).unwrap());
        assert_eq!(
            s.schema_info(0x8000_0001).unwrap(),
            Some(("local".into(), 1))
        );
        let row = s.schema_row(0x8000_0001, "a").unwrap().unwrap();
        assert_eq!(row.display_name, "Alpha");
        assert_eq!(row.description, "Alpha description");
        assert!(s.replace_local_schema(0x8000_0001, &texts[..1], 3).unwrap());
        s.replace_schema(7, "english", Some("G"), &[row_named("X")], 1)
            .unwrap();
        assert!(!s.replace_local_schema(7, &texts, 2).unwrap());
        assert_eq!(s.schema_info(7).unwrap(), Some(("english".into(), 1)));
    }

    fn row_named(api: &str) -> SchemaRow {
        SchemaRow {
            api_name: api.into(),
            display_name: api.into(),
            description: String::new(),
            icon: None,
            icon_gray: None,
            hidden: false,
            percent: None,
        }
    }

    #[test]
    fn old_database_gains_external_games_table() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("old.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE games (app_id INTEGER PRIMARY KEY, name TEXT, schema_lang TEXT, schema_fetched_at INTEGER);")
            .unwrap();
        let s = Store::open(&path).unwrap();
        assert!(s.external_games().unwrap().is_empty());
    }

    #[test]
    fn replace_schema_works_inside_an_open_transaction() {
        let s = Store::in_memory().unwrap();
        let tx = s.begin().unwrap();
        s.replace_schema(7, "english", Some("G"), &[row("A", 1.0)], 1)
            .unwrap();
        tx.commit().unwrap();
        assert_eq!(s.schema_names(7).unwrap(), vec!["A".to_owned()]);
    }
}
