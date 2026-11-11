//! SQLite cache in `/cache/phn-cache.sqlite`.
//!
//! Two tables:
//! * `http_cache`: bodies of live responses with a time-to-live, so a later
//!   call can fall back to the cached copy when the API is unreachable.
//! * `source_status`: the latest reachability check for each data source.
//!
//! Bodies fetched more than [`MAX_AGE_DAYS`] ago are deleted when the cache
//! is opened, so the file does not grow without limit.

use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use phn_core::{PhnError, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::http::ProbeResult;

/// A cached HTTP body.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedBody {
    pub body: Vec<u8>,
    pub fetched_at: DateTime<Utc>,
    /// True when the entry is younger than its time-to-live.
    pub fresh: bool,
}

/// Cached bodies older than this are deleted when the cache is opened. The
/// longest time-to-live in use is 30 days, so nothing still fresh is removed.
pub const MAX_AGE_DAYS: i64 = 90;

/// SQLite-backed cache. Cheap operations run inline under a mutex.
#[derive(Debug)]
pub struct Cache {
    conn: Mutex<Connection>,
}

fn db_err(e: rusqlite::Error) -> PhnError {
    PhnError::Internal(format!("cache database: {e}"))
}

impl Cache {
    /// Open (or create) the cache database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).map_err(db_err)?;
        Self::init(conn)
    }

    /// An in-memory cache (tests, or when `/cache` is not writable).
    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory().map_err(db_err)?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS http_cache (
                 key TEXT PRIMARY KEY,
                 body BLOB NOT NULL,
                 fetched_at TEXT NOT NULL,
                 ttl_secs INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS source_status (
                 source_id TEXT PRIMARY KEY,
                 result_json TEXT NOT NULL
             );",
        )
        .map_err(db_err)?;
        let cache = Self {
            conn: Mutex::new(conn),
        };
        if let Err(e) = cache.prune(Utc::now()) {
            tracing::warn!(error = %e, "could not prune the HTTP cache");
        }
        Ok(cache)
    }

    /// Delete bodies fetched more than [`MAX_AGE_DAYS`] before `now`. Returns
    /// the number of bodies removed.
    pub fn prune(&self, now: DateTime<Utc>) -> Result<usize> {
        // `fetched_at` is always written as UTC RFC 3339 text, which sorts
        // in time order.
        let cutoff = (now - chrono::Duration::days(MAX_AGE_DAYS)).to_rfc3339();
        self.lock()
            .execute(
                "DELETE FROM http_cache WHERE fetched_at < ?1",
                params![cutoff],
            )
            .map_err(db_err)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Store a body with a time-to-live.
    pub fn put(&self, key: &str, body: &[u8], ttl_secs: u64) -> Result<()> {
        self.put_at(key, body, ttl_secs, Utc::now())
    }

    /// Store a body with an explicit fetch time (tests).
    pub fn put_at(&self, key: &str, body: &[u8], ttl_secs: u64, at: DateTime<Utc>) -> Result<()> {
        self.lock()
            .execute(
                "INSERT INTO http_cache (key, body, fetched_at, ttl_secs) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(key) DO UPDATE SET body = excluded.body,
                     fetched_at = excluded.fetched_at, ttl_secs = excluded.ttl_secs",
                params![key, body, at.to_rfc3339(), ttl_secs as i64],
            )
            .map_err(db_err)?;
        Ok(())
    }

    /// Read a cached body, fresh or stale.
    pub fn get(&self, key: &str) -> Result<Option<CachedBody>> {
        let row: Option<(Vec<u8>, String, i64)> = self
            .lock()
            .query_row(
                "SELECT body, fetched_at, ttl_secs FROM http_cache WHERE key = ?1",
                params![key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(db_err)?;
        Ok(row.and_then(|(body, at, ttl)| {
            let fetched_at = DateTime::parse_from_rfc3339(&at).ok()?.with_timezone(&Utc);
            let age = Utc::now().signed_duration_since(fetched_at).num_seconds();
            Some(CachedBody {
                body,
                fetched_at,
                fresh: age >= 0 && age < ttl,
            })
        }))
    }

    /// Read a cached body, treating a read error as a miss. The error is
    /// logged (to stderr) so a damaged cache never blocks the live API or the
    /// bundled snapshot.
    pub fn get_or_miss(&self, key: &str) -> Option<CachedBody> {
        match self.get(key) {
            Ok(hit) => hit,
            Err(e) => {
                tracing::warn!(error = %e, "could not read the HTTP cache; treating it as a miss");
                None
            }
        }
    }

    /// Make every later read and write fail, as a damaged cache file would
    /// (for tests of the fallback chain).
    #[cfg(test)]
    pub(crate) fn damage_for_tests(&self) {
        self.lock()
            .execute_batch("DROP TABLE http_cache")
            .expect("drop the cache table");
    }

    /// Record the latest reachability check for a source.
    pub fn record_probe(&self, source_id: &str, result: &ProbeResult) -> Result<()> {
        let json = serde_json::to_string(result)
            .map_err(|e| PhnError::Internal(format!("serialize probe: {e}")))?;
        self.lock()
            .execute(
                "INSERT INTO source_status (source_id, result_json) VALUES (?1, ?2)
                 ON CONFLICT(source_id) DO UPDATE SET result_json = excluded.result_json",
                params![source_id, json],
            )
            .map_err(db_err)?;
        Ok(())
    }

    /// The latest recorded check for a source.
    pub fn last_probe(&self, source_id: &str) -> Result<Option<ProbeResult>> {
        let json: Option<String> = self
            .lock()
            .query_row(
                "SELECT result_json FROM source_status WHERE source_id = ?1",
                params![source_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_expires_bodies() {
        let c = Cache::in_memory().unwrap();
        assert!(c.get("k").unwrap().is_none());
        c.put("k", b"hello", 3600).unwrap();
        let hit = c.get("k").unwrap().unwrap();
        assert_eq!(hit.body, b"hello");
        assert!(hit.fresh);
        c.put_at("old", b"x", 60, Utc::now() - chrono::Duration::hours(2))
            .unwrap();
        assert!(!c.get("old").unwrap().unwrap().fresh);
    }

    #[test]
    fn read_errors_are_treated_as_a_miss() {
        let c = Cache::in_memory().unwrap();
        c.put("k", b"hello", 3600).unwrap();
        assert!(c.get_or_miss("k").is_some());
        // Simulate a damaged cache: the table is gone.
        c.damage_for_tests();
        assert!(c.get("k").is_err());
        assert!(c.get_or_miss("k").is_none());
    }

    #[test]
    fn old_bodies_are_pruned_when_the_cache_opens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        let c = Cache::open(&path).unwrap();
        let now = Utc::now();
        c.put_at(
            "ancient",
            b"x",
            60,
            now - chrono::Duration::days(MAX_AGE_DAYS + 10),
        )
        .unwrap();
        c.put_at(
            "stale",
            b"y",
            60,
            now - chrono::Duration::days(MAX_AGE_DAYS - 10),
        )
        .unwrap();
        c.put("fresh", b"z", 60).unwrap();
        drop(c);
        let c = Cache::open(&path).unwrap();
        assert!(c.get("ancient").unwrap().is_none());
        // A stale copy inside the limit is kept as a fallback.
        assert!(!c.get("stale").unwrap().unwrap().fresh);
        assert!(c.get("fresh").unwrap().unwrap().fresh);
        assert_eq!(
            c.prune(now + chrono::Duration::days(MAX_AGE_DAYS + 1))
                .unwrap(),
            2
        );
    }

    #[test]
    fn records_probes_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/cache.sqlite");
        let c = Cache::open(&path).unwrap();
        let p = ProbeResult {
            reachable: true,
            http_status: Some(200),
            latency_ms: Some(12),
            error: None,
            checked_at: "2026-01-01T00:00:00Z".into(),
        };
        c.record_probe("src", &p).unwrap();
        drop(c);
        let c = Cache::open(&path).unwrap();
        assert_eq!(c.last_probe("src").unwrap(), Some(p));
    }
}
