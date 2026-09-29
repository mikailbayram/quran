//! Local SQLite store: HTTP response cache, settings, bookmarks, reading progress.

use crate::models::{Bookmark, LastRead};
use crate::settings::Settings;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Store {
    conn: Mutex<Connection>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA temp_store=MEMORY;
             CREATE TABLE IF NOT EXISTS http_cache (
                 url TEXT PRIMARY KEY,
                 body BLOB NOT NULL,
                 fetched_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS kv (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS bookmarks (
                 verse_key TEXT PRIMARY KEY,
                 created_at INTEGER NOT NULL
             );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn cache_get(&self, url: &str) -> Option<Vec<u8>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT body FROM http_cache WHERE url = ?1",
            params![url],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten()
    }

    pub fn cache_put(&self, url: &str, body: &[u8]) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO http_cache (url, body, fetched_at) VALUES (?1, ?2, ?3)",
            params![url, body, now()],
        );
    }

    pub fn cache_delete(&self, url: &str) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM http_cache WHERE url = ?1", params![url]);
    }

    pub fn cache_size(&self) -> u64 {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(body)), 0) FROM http_cache",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as u64
    }

    pub fn cache_clear(&self) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM http_cache", []);
        let _ = conn.execute("VACUUM", []);
    }

    fn kv_get(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT value FROM kv WHERE key = ?1", params![key], |r| {
            r.get(0)
        })
        .optional()
        .ok()
        .flatten()
    }

    fn kv_set(&self, key: &str, value: &str) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)",
            params![key, value],
        );
    }

    pub fn settings(&self) -> Settings {
        self.kv_get("settings")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) {
        if let Ok(json) = serde_json::to_string(s) {
            self.kv_set("settings", &json);
        }
    }

    pub fn last_read(&self) -> Option<LastRead> {
        self.kv_get("last_read")
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn set_last_read(&self, lr: &LastRead) {
        if let Ok(json) = serde_json::to_string(lr) {
            self.kv_set("last_read", &json);
        }
    }

    pub fn bookmarks(&self) -> Vec<Bookmark> {
        let conn = self.conn.lock().unwrap();
        let Ok(mut stmt) =
            conn.prepare("SELECT verse_key, created_at FROM bookmarks ORDER BY created_at DESC")
        else {
            return vec![];
        };
        stmt.query_map([], |r| {
            Ok(Bookmark {
                verse_key: r.get(0)?,
                created_at: r.get(1)?,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }

    pub fn is_bookmarked(&self, verse_key: &str) -> bool {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT 1 FROM bookmarks WHERE verse_key = ?1",
            params![verse_key],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some()
    }

    /// Toggle and return the new state.
    pub fn toggle_bookmark(&self, verse_key: &str) -> bool {
        let on = self.is_bookmarked(verse_key);
        let conn = self.conn.lock().unwrap();
        if on {
            let _ = conn.execute(
                "DELETE FROM bookmarks WHERE verse_key = ?1",
                params![verse_key],
            );
        } else {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO bookmarks (verse_key, created_at) VALUES (?1, ?2)",
                params![verse_key, now()],
            );
        }
        !on
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let s = Store::in_memory().unwrap();
        assert!(s.cache_get("x").is_none());
        s.cache_put("x", b"hello");
        assert_eq!(s.cache_get("x").unwrap(), b"hello");
        assert!(s.toggle_bookmark("2:255"));
        assert!(s.is_bookmarked("2:255"));
        assert_eq!(s.bookmarks().len(), 1);
        assert!(!s.toggle_bookmark("2:255"));
        let mut st = s.settings();
        st.reciter = 3;
        s.save_settings(&st);
        assert_eq!(s.settings().reciter, 3);
    }
}
