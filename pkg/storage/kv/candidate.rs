// Copyright 2026 AsterSQL.
//! SQLite disk boundary for the native consensus experiment.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

use fs2::FileExt;

#[cfg(test)]
use rusqlite::OpenFlags;
use rusqlite::{Connection, params};

pub(super) struct Candidate {
    pub(super) db: Connection,
    _owner: Option<File>,
}

impl Candidate {
    #[cfg(test)]
    pub(super) fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::open_using(path, None)
    }

    #[cfg(test)]
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
        Self::sync_config(&db)?;
        db.execute_batch(&format!("BEGIN; {SCHEMA} COMMIT;"))?;
        Ok(Self { db, _owner: None })
    }

    pub(super) fn open_native(
        directory: &Path,
        cluster: u64,
        node: u64,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if cluster == 0 || node == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "zero cluster or node identity",
            )
            .into());
        }
        fs::create_dir_all(directory)?;
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("metadata.lock"))?;
        FileExt::try_lock_exclusive(&owner)?;
        let mut db = Connection::open(directory.join("metadata.sqlite"))?;
        let expected = serde_json::to_vec(&("astersql-pd-native", 1_u32, cluster, node))?;
        let tables: i64 =
            db.query_row("SELECT count(*) FROM sqlite_schema", [], |row| row.get(0))?;
        if tables != 0 {
            let actual: Vec<u8> = db.query_row(
                "SELECT payload FROM state WHERE name='native_identity'",
                [],
                |row| row.get(0),
            )?;
            if actual != expected {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unknown format or wrong metadata identity",
                )
                .into());
            }
            db.prepare("SELECT idx, payload FROM log_entries LIMIT 0")?;
        }
        Self::sync_config(&db)?;
        if tables == 0 {
            let tx = db.transaction()?;
            tx.execute_batch(SCHEMA)?;
            tx.execute(
                "INSERT INTO state VALUES ('native_identity', ?1)",
                params![expected],
            )?;
            tx.commit()?;
            #[cfg(unix)]
            File::open(directory)?.sync_all()?;
        }
        Ok(Self {
            db,
            _owner: Some(owner),
        })
    }

    fn sync_config(db: &Connection) -> rusqlite::Result<()> {
        // etcd uses F_FULLFSYNC on macOS. Request the same device-cache
        // barrier from SQLite's native VFS before opening its WAL.
        #[cfg(target_os = "macos")]
        db.pragma_update(None, "fullfsync", true)?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        let journal: String = db.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        let sync: i64 = db.pragma_query_value(None, "synchronous", |row| row.get(0))?;
        if journal != "wal" || sync != 2 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(())
    }
}

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS state (name TEXT PRIMARY KEY, payload BLOB NOT NULL);
    CREATE TABLE IF NOT EXISTS log_entries (idx INTEGER PRIMARY KEY, payload BLOB NOT NULL);
    CREATE TABLE IF NOT EXISTS records (key BLOB PRIMARY KEY, value BLOB NOT NULL);
";
