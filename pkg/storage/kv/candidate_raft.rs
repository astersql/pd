// Copyright 2026 AsterSQL.
//! SQLite implementation of the storage contracts exercised by the OpenRaft experiment.

use std::fmt::Debug;
use std::io::{self, Cursor};
use std::ops::{Bound, RangeBounds};
use std::sync::{Arc, Mutex};

use openraft::storage::{LogFlushed, RaftLogStorage, RaftStateMachine};
use openraft::{
    AnyError, BasicNode, Entry, EntryPayload, ErrorSubject, ErrorVerb, LogId, LogState,
    RaftLogReader, RaftSnapshotBuilder, Snapshot, SnapshotMeta, StorageError, StorageIOError,
    StoredMembership, Vote,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::candidate::Candidate;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct PutPair(pub [(Vec<u8>, Vec<u8>); 2]);

openraft::declare_raft_types!(pub(super) Type: D = PutPair, R = u64);

type Local<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
type StoreResult<T> = Result<T, StorageError<u64>>;
type Membership = StoredMembership<u64, BasicNode>;
type Meta = SnapshotMeta<u64, BasicNode>;
type Records = Vec<(Vec<u8>, Vec<u8>)>;

#[derive(Deserialize, Serialize)]
struct Image {
    meta: Meta,
    records: Records,
    revision: u64,
}

#[derive(Clone)]
pub(super) struct RaftDisk(Arc<Mutex<Connection>>);

fn failure(verb: ErrorVerb, error: impl std::fmt::Display) -> StorageError<u64> {
    StorageIOError::new(ErrorSubject::Store, verb, AnyError::error(error)).into()
}

fn load<T: DeserializeOwned>(db: &Connection, key: &str) -> Local<Option<T>> {
    let bytes: Option<Vec<u8>> = db
        .query_row("SELECT payload FROM state WHERE name=?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    bytes
        .map(|value| serde_json::from_slice(&value).map_err(Into::into))
        .transpose()
}

fn save<T: Serialize>(db: &Connection, key: &str, value: &T) -> Local<()> {
    db.execute(
        "INSERT OR REPLACE INTO state VALUES (?1, ?2)",
        params![key, serde_json::to_vec(value)?],
    )?;
    Ok(())
}

impl RaftDisk {
    pub(super) fn new(candidate: Candidate) -> Self {
        Self(Arc::new(Mutex::new(candidate.db)))
    }

    fn read<T>(&self, f: impl FnOnce(&Connection) -> Local<T>) -> StoreResult<T> {
        let db = self.0.lock().map_err(|e| failure(ErrorVerb::Read, e))?;
        f(&db).map_err(|e| failure(ErrorVerb::Read, e))
    }

    fn write<T>(&self, f: impl FnOnce(&Connection) -> Local<T>) -> StoreResult<T> {
        let mut db = self.0.lock().map_err(|e| failure(ErrorVerb::Write, e))?;
        let tx = db.transaction().map_err(|e| failure(ErrorVerb::Write, e))?;
        let result = f(&tx).map_err(|e| failure(ErrorVerb::Write, e))?;
        tx.commit().map_err(|e| failure(ErrorVerb::Write, e))?;
        Ok(result)
    }

    pub(super) fn value(&self, key: &[u8]) -> StoreResult<Option<Vec<u8>>> {
        self.read(|db| {
            Ok(db
                .query_row("SELECT value FROM records WHERE key=?1", [key], |r| {
                    r.get(0)
                })
                .optional()?)
        })
    }
}

impl RaftLogReader<Type> for RaftDisk {
    async fn try_get_log_entries<R: RangeBounds<u64> + Clone + Debug + Send>(
        &mut self,
        range: R,
    ) -> StoreResult<Vec<Entry<Type>>> {
        let lower = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.saturating_add(1),
            Bound::Unbounded => 0,
        };
        let Ok(lower) = i64::try_from(lower) else {
            return Ok(Vec::new());
        };
        let upper = match range.end_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => match n.checked_sub(1) {
                Some(n) => n,
                None => return Ok(Vec::new()),
            },
            Bound::Unbounded => u64::MAX,
        };
        let upper = i64::try_from(upper).unwrap_or(i64::MAX);
        self.read(|db| {
            let mut stmt = db.prepare(
                "SELECT payload FROM log_entries WHERE idx BETWEEN ?1 AND ?2 ORDER BY idx",
            )?;
            let rows = stmt.query_map(params![lower, upper], |row| row.get::<_, Vec<u8>>(0))?;
            rows.map(|value| Ok(serde_json::from_slice(&value?)?))
                .collect()
        })
    }
}

impl RaftLogStorage<Type> for RaftDisk {
    type LogReader = Self;

    async fn get_log_state(&mut self) -> StoreResult<LogState<Type>> {
        self.read(|db| {
            let last_purged_log_id = load(db, "purged")?;
            let payload: Option<Vec<u8>> = db
                .query_row(
                    "SELECT payload FROM log_entries ORDER BY idx DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            let last_log_id = match payload {
                Some(data) => Some(serde_json::from_slice::<Entry<Type>>(&data)?.log_id),
                None => last_purged_log_id,
            };
            Ok(LogState {
                last_purged_log_id,
                last_log_id,
            })
        })
    }

    async fn get_log_reader(&mut self) -> Self {
        self.clone()
    }

    async fn save_vote(&mut self, vote: &Vote<u64>) -> StoreResult<()> {
        self.write(|db| save(db, "vote", vote))
    }

    async fn read_vote(&mut self) -> StoreResult<Option<Vote<u64>>> {
        self.read(|db| load(db, "vote"))
    }

    async fn save_committed(&mut self, committed: Option<LogId<u64>>) -> StoreResult<()> {
        self.write(|db| save(db, "committed", &committed))
    }

    async fn read_committed(&mut self) -> StoreResult<Option<LogId<u64>>> {
        self.read(|db| Ok(load::<Option<LogId<u64>>>(db, "committed")?.flatten()))
    }

    async fn append<I>(&mut self, entries: I, callback: LogFlushed<Type>) -> StoreResult<()>
    where
        I: IntoIterator<Item = Entry<Type>> + Send,
        I::IntoIter: Send,
    {
        let result = self.write(|db| {
            let mut stmt = db.prepare("INSERT INTO log_entries VALUES (?1, ?2)")?;
            for entry in entries {
                stmt.execute(params![
                    i64::try_from(entry.log_id.index)?,
                    serde_json::to_vec(&entry)?
                ])?;
            }
            Ok(())
        });
        callback.log_io_completed(
            result
                .as_ref()
                .map(|_| ())
                .map_err(|err| io::Error::other(err.to_string())),
        );
        result
    }

    async fn truncate(&mut self, log_id: LogId<u64>) -> StoreResult<()> {
        self.write(|db| {
            db.execute(
                "DELETE FROM log_entries WHERE idx>=?1",
                [i64::try_from(log_id.index)?],
            )?;
            Ok(())
        })
    }

    async fn purge(&mut self, log_id: LogId<u64>) -> StoreResult<()> {
        self.write(|db| {
            db.execute(
                "DELETE FROM log_entries WHERE idx<=?1",
                [i64::try_from(log_id.index)?],
            )?;
            save(db, "purged", &log_id)
        })
    }
}

impl RaftStateMachine<Type> for RaftDisk {
    type SnapshotBuilder = Self;

    async fn applied_state(&mut self) -> StoreResult<(Option<LogId<u64>>, Membership)> {
        self.read(|db| {
            Ok((
                load(db, "applied")?,
                load(db, "membership")?.unwrap_or_default(),
            ))
        })
    }

    async fn apply<I>(&mut self, entries: I) -> StoreResult<Vec<u64>>
    where
        I: IntoIterator<Item = Entry<Type>> + Send,
        I::IntoIter: Send,
    {
        self.write(|db| {
            let mut revision = load::<u64>(db, "revision")?.unwrap_or(0);
            let mut replies = Vec::new();
            for entry in entries {
                match entry.payload {
                    EntryPayload::Blank => {}
                    EntryPayload::Membership(membership) => save(
                        db,
                        "membership",
                        &Membership::new(Some(entry.log_id), membership),
                    )?,
                    EntryPayload::Normal(PutPair(records)) => {
                        revision = revision
                            .checked_add(1)
                            .ok_or_else(|| io::Error::other("revision exhausted"))?;
                        for (key, value) in records {
                            db.execute(
                                "INSERT OR REPLACE INTO records VALUES (?1, ?2)",
                                params![key, value],
                            )?;
                        }
                        save(db, "revision", &revision)?;
                    }
                }
                save(db, "applied", &entry.log_id)?;
                replies.push(revision);
            }
            Ok(replies)
        })
    }

    async fn get_snapshot_builder(&mut self) -> Self {
        self.clone()
    }

    async fn begin_receiving_snapshot(&mut self) -> StoreResult<Box<Cursor<Vec<u8>>>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &Meta,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> StoreResult<()> {
        let image: Image =
            serde_json::from_slice(snapshot.get_ref()).map_err(|e| failure(ErrorVerb::Read, e))?;
        if &image.meta != meta {
            return Err(failure(ErrorVerb::Read, "snapshot metadata mismatch"));
        }
        self.write(|db| {
            db.execute("DELETE FROM records", [])?;
            for (key, value) in &image.records {
                db.execute("INSERT INTO records VALUES (?1, ?2)", params![key, value])?;
            }
            if let Some(applied) = meta.last_log_id {
                save(db, "applied", &applied)?;
            } else {
                db.execute("DELETE FROM state WHERE name='applied'", [])?;
            }
            save(db, "revision", &image.revision)?;
            save(db, "membership", &meta.last_membership)?;
            save(db, "snapshot", &image)
        })
    }

    async fn get_current_snapshot(&mut self) -> StoreResult<Option<Snapshot<Type>>> {
        self.read(|db| load::<Image>(db, "snapshot")?.map(to_snapshot).transpose())
    }
}

fn to_snapshot(image: Image) -> Local<Snapshot<Type>> {
    let data = serde_json::to_vec(&image)?;
    Ok(Snapshot {
        meta: image.meta,
        snapshot: Box::new(Cursor::new(data)),
    })
}

impl RaftSnapshotBuilder<Type> for RaftDisk {
    async fn build_snapshot(&mut self) -> StoreResult<Snapshot<Type>> {
        self.write(|db| {
            let applied = load::<LogId<u64>>(db, "applied")?;
            let meta = Meta {
                snapshot_id: applied.map_or_else(|| "empty".into(), |id| id.to_string()),
                last_log_id: applied,
                last_membership: load(db, "membership")?.unwrap_or_default(),
            };
            let mut stmt = db.prepare("SELECT key, value FROM records ORDER BY key")?;
            let records = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let image = Image {
                meta,
                records,
                revision: load(db, "revision")?.unwrap_or(0),
            };
            save(db, "snapshot", &image)?;
            to_snapshot(image)
        })
    }
}
