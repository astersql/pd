// Copyright 2026 AsterSQL.
//! SQLite disk boundary for the native consensus experiment.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

pub(super) struct Candidate {
    pub(super) db: Connection,
}

impl Candidate {
    pub(super) fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::open_using(path, None)
    }

    pub(super) fn open_using(path: &Path, vfs: Option<&str>) -> rusqlite::Result<Self> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
        let db = match vfs {
            Some(vfs) => {
                let db = Connection::open_with_flags_and_vfs(path, flags, vfs)?;
                db.pragma_update(None, "locking_mode", "EXCLUSIVE")?;
                db
            }
            None => Connection::open_with_flags(path, flags)?,
        };
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        let journal: String = db.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        let sync: i64 = db.pragma_query_value(None, "synchronous", |row| row.get(0))?;
        if journal != "wal" || sync != 2 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        db.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS state (name TEXT PRIMARY KEY, payload BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS log_entries (idx INTEGER PRIMARY KEY, payload BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS records (key BLOB PRIMARY KEY, value BLOB NOT NULL);
             COMMIT;",
        )?;
        Ok(Self { db })
    }
}
