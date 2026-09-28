//! Durable metadata index for fast recent-session lists.
//!
//! Transcript snapshots can be hundreds of megabytes and a long-lived install
//! can contain 100k+ files. This SQLite index is updated beside normal session
//! persistence and can be queried across daemon, CLI, and API bridge processes.

use std::time::Duration;

use anyhow::Result;
use rusqlite::{Connection, params};

use crate::session::Session;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentSessionMetadata {
    pub session_id: String,
    pub working_dir: Option<String>,
    pub saved: bool,
    pub updated_at_ms: i64,
    pub last_active_at_ms: Option<i64>,
}

fn open() -> Result<Connection> {
    let path = crate::storage::kcode_dir()?.join("session-metadata-v1.sqlite3");
    let connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(2))?;
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         CREATE TABLE IF NOT EXISTS recent_sessions (
             session_id TEXT PRIMARY KEY NOT NULL,
             working_dir TEXT,
             updated_at_ms INTEGER NOT NULL,
             last_active_at_ms INTEGER
         );
         CREATE INDEX IF NOT EXISTS recent_sessions_activity
         ON recent_sessions(COALESCE(last_active_at_ms, updated_at_ms) DESC);",
    )?;
    // Additive migration for databases created before saved-session ordering
    // became part of the shared session-list contract.
    let _ = connection.execute(
        "ALTER TABLE recent_sessions ADD COLUMN saved INTEGER NOT NULL DEFAULT 0",
        [],
    );
    Ok(connection)
}

pub fn recent(limit: usize) -> Result<Vec<RecentSessionMetadata>> {
    let connection = open()?;
    let mut statement = connection.prepare(
        "SELECT session_id, working_dir, saved, updated_at_ms, last_active_at_ms
         FROM recent_sessions
         ORDER BY COALESCE(last_active_at_ms, updated_at_ms) DESC
         LIMIT ?1",
    )?;
    let entries = statement
        .query_map([i64::try_from(limit).unwrap_or(i64::MAX)], |row| {
            Ok(RecentSessionMetadata {
                session_id: row.get(0)?,
                working_dir: row.get(1)?,
                saved: row.get(2)?,
                updated_at_ms: row.get(3)?,
                last_active_at_ms: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(entries)
}

/// Update the index after a successful session persistence operation.
pub fn upsert_session(session: &Session) -> Result<()> {
    upsert(&RecentSessionMetadata {
        session_id: session.id.clone(),
        working_dir: session.working_dir.clone(),
        saved: session.saved,
        updated_at_ms: session.updated_at.timestamp_millis(),
        last_active_at_ms: session.last_active_at.map(|time| time.timestamp_millis()),
    })
}

pub fn upsert(entry: &RecentSessionMetadata) -> Result<()> {
    open()?.execute(
        "INSERT INTO recent_sessions (
             session_id, working_dir, saved, updated_at_ms, last_active_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(session_id) DO UPDATE SET
             working_dir = excluded.working_dir,
             saved = excluded.saved,
             updated_at_ms = excluded.updated_at_ms,
             last_active_at_ms = excluded.last_active_at_ms",
        params![
            entry.session_id,
            entry.working_dir,
            entry.saved,
            entry.updated_at_ms,
            entry.last_active_at_ms,
        ],
    )?;
    Ok(())
}
