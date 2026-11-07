// Copyright 2026 AsterSQL.
//! Experimental state machine derived from durable snapshots and quorum logs.

use openraft::{EntryPayload, RaftSnapshotBuilder, storage::RaftStateMachine};
#[cfg(test)]
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::Notify;

use super::*;

#[derive(Default)]
pub(super) struct InstallFence {
    receiving: AtomicBool,
    changed: Notify,
}

impl InstallFence {
    pub(super) async fn wait_for_snapshot<C: DiskConfig>(
        &self,
        disk: &SqliteRaftLog<C>,
        log_id: LogId<u64>,
    ) -> StoreResult<()> {
        let wait = async {
            loop {
                let mut changed = std::pin::pin!(self.changed.notified());
                changed.as_mut().enable();
                let receiving = self.receiving.load(Ordering::Acquire);
                let covered = disk.read(|db| {
                    Ok(load::<SnapshotEnvelope>(db, "snapshot")?
                        .map(|snapshot| snapshot.image::<SnapshotHeader>())
                        .transpose()?
                        .and_then(|snapshot| snapshot.meta.last_log_id)
                        .is_some_and(|snapshot_id| snapshot_id.index >= log_id.index))
                })?;
                if covered {
                    return Ok(());
                }
                if !receiving {
                    return Err(failure(
                        ErrorVerb::Write,
                        "cannot purge replay log without a durable snapshot",
                    ));
                }
                changed.await;
            }
        };
        tokio::time::timeout(Duration::from_secs(5), wait)
            .await
            .map_err(|_| {
                failure(
                    ErrorVerb::Write,
                    "snapshot installation did not complete before purge",
                )
            })?
    }

    fn complete(&self) {
        self.receiving.store(false, Ordering::Release);
        self.changed.notify_waiters();
    }
}

/// Deterministic application of a committed command. Only the snapshot and log
/// may supply state on reopen. External observations belong in the command.
pub(crate) trait ReplayApplication<C: DiskConfig>:
    Default + Send + Serialize + DeserializeOwned + 'static
{
    fn apply(&mut self, command: C::D) -> StoreResult<C::R>;
}

#[derive(Deserialize)]
struct SnapshotHeader {
    meta: Meta,
}

#[derive(Deserialize, Serialize)]
struct ReplayImage<A> {
    meta: Meta,
    #[serde(flatten)]
    app: A,
}

pub(crate) struct ReplayMachine<C, A> {
    disk: SqliteRaftLog<C>,
    state: Arc<Mutex<ReplayImage<A>>>,
    changed: Arc<Notify>,
}

impl<C: DiskConfig, A> Clone for ReplayMachine<C, A> {
    fn clone(&self) -> Self {
        Self {
            disk: self.disk.clone(),
            state: self.state.clone(),
            changed: self.changed.clone(),
        }
    }
}

impl<C: DiskConfig, A: ReplayApplication<C>> ReplayMachine<C, A> {
    pub(crate) fn open(mut disk: SqliteRaftLog<C>) -> StoreResult<(SqliteRaftLog<C>, Self)> {
        let state = disk.read(|db| {
            if load::<LogId<u64>>(db, "applied")?.is_some() {
                return Err(io::Error::other("replay requires a replay-format database").into());
            }
            Ok(load::<SnapshotEnvelope>(db, "snapshot")?
                .map(|snapshot| snapshot.image())
                .transpose()?
                .unwrap_or_else(|| ReplayImage {
                    meta: Meta {
                        last_log_id: None,
                        last_membership: Membership::default(),
                        snapshot_id: "empty".into(),
                    },
                    app: A::default(),
                }))
        })?;
        disk.1 = Some(Arc::new(InstallFence::default()));
        Ok((
            disk.clone(),
            Self {
                disk,
                state: Arc::new(Mutex::new(state)),
                changed: Arc::new(Notify::new()),
            },
        ))
    }

    pub(crate) fn inspect<T>(&self, f: impl FnOnce(&A) -> T) -> StoreResult<T> {
        let state = self.state.lock().map_err(|e| failure(ErrorVerb::Read, e))?;
        Ok(f(&state.app))
    }

    pub(crate) fn changed(&self) -> &Notify {
        &self.changed
    }
}

#[cfg(test)]
#[derive(Default, Deserialize, Serialize)]
pub(crate) struct PairState {
    #[serde(with = "record_list")]
    records: BTreeMap<Vec<u8>, Vec<u8>>,
    revision: u64,
}

#[cfg(test)]
mod record_list {
    use super::*;
    pub(super) fn serialize<S: serde::Serializer>(
        records: &BTreeMap<Vec<u8>, Vec<u8>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        records.iter().collect::<Vec<_>>().serialize(serializer)
    }
    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, D::Error> {
        Records::deserialize(deserializer).map(|records| records.into_iter().collect())
    }
}

#[cfg(test)]
impl ReplayApplication<Type> for PairState {
    fn apply(&mut self, PutPair(records): PutPair) -> StoreResult<u64> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| failure(ErrorVerb::Write, "revision exhausted"))?;
        self.records.extend(records);
        Ok(self.revision)
    }
}

#[cfg(test)]
impl ReplayMachine<Type, PairState> {
    // Callers must first establish the leader's read barrier. A reopened local
    // snapshot can be older than the latest acknowledged transaction.
    pub(crate) fn value(&self, key: &[u8]) -> StoreResult<Option<Vec<u8>>> {
        self.inspect(|state| state.records.get(key).cloned())
    }

    pub(crate) fn read_pair(&self, keys: [&[u8]; 2]) -> StoreResult<(u64, [Option<Vec<u8>>; 2])> {
        self.inspect(|state| {
            (
                state.revision,
                keys.map(|key| state.records.get(key).cloned()),
            )
        })
    }
}

impl<C: DiskConfig, A: ReplayApplication<C>> RaftStateMachine<C> for ReplayMachine<C, A>
where
    C::R: Default,
{
    type SnapshotBuilder = Self;

    async fn applied_state(&mut self) -> StoreResult<(Option<LogId<u64>>, Membership)> {
        let state = self.state.lock().map_err(|e| failure(ErrorVerb::Read, e))?;
        Ok((state.meta.last_log_id, state.meta.last_membership.clone()))
    }

    async fn apply<I>(&mut self, entries: I) -> StoreResult<Vec<C::R>>
    where
        I: IntoIterator<Item = Entry<C>> + Send,
        I::IntoIter: Send,
    {
        let started = Instant::now();
        let mut state = self
            .state
            .lock()
            .map_err(|e| failure(ErrorVerb::Write, e))?;
        let entries: Vec<_> = entries.into_iter().collect();
        let mut previous = state.meta.last_log_id;
        for entry in &entries {
            if previous.map_or(Some(0), |id| id.index.checked_add(1)) != Some(entry.log_id.index) {
                return Err(failure(ErrorVerb::Write, "non-contiguous replay log"));
            }
            previous = Some(entry.log_id);
        }
        let mut replies = Vec::with_capacity(entries.len());
        for entry in entries {
            let reply = match entry.payload {
                EntryPayload::Blank => C::R::default(),
                EntryPayload::Membership(membership) => {
                    state.meta.last_membership = Membership::new(Some(entry.log_id), membership);
                    C::R::default()
                }
                EntryPayload::Normal(command) => state.app.apply(command)?,
            };
            state.meta.last_log_id = Some(entry.log_id);
            replies.push(reply);
        }
        event(
            "apply",
            state.meta.last_log_id.map_or(0, |id| id.index),
            started,
        );
        self.changed.notify_waiters();
        Ok(replies)
    }

    async fn get_snapshot_builder(&mut self) -> Self {
        self.clone()
    }

    async fn begin_receiving_snapshot(&mut self) -> StoreResult<Box<Cursor<Vec<u8>>>> {
        self.disk
            .1
            .as_ref()
            .unwrap()
            .receiving
            .store(true, Ordering::Release);
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &Meta,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> StoreResult<()> {
        let result = (|| {
            let envelope: SnapshotEnvelope = serde_json::from_slice(snapshot.get_ref())
                .map_err(|e| failure(ErrorVerb::Read, e))?;
            let image: ReplayImage<A> =
                envelope.image().map_err(|e| failure(ErrorVerb::Read, e))?;
            if &image.meta != meta {
                return Err(failure(ErrorVerb::Read, "snapshot metadata mismatch"));
            }
            let mut state = self
                .state
                .lock()
                .map_err(|e| failure(ErrorVerb::Write, e))?;
            if state.meta.last_log_id > image.meta.last_log_id {
                return Err(failure(
                    ErrorVerb::Write,
                    "snapshot would roll back applied state",
                ));
            }
            self.disk.write(|db| save(db, "snapshot", &envelope))?;
            *state = image;
            self.changed.notify_waiters();
            Ok(())
        })();
        self.disk.1.as_ref().unwrap().complete();
        result
    }

    async fn get_current_snapshot(&mut self) -> StoreResult<Option<Snapshot<C>>> {
        self.disk.read(|db| {
            let Some(envelope) = load::<SnapshotEnvelope>(db, "snapshot")? else {
                return Ok(None);
            };
            let meta = envelope.image::<SnapshotHeader>()?.meta;
            envelope.into_snapshot(meta).map(Some)
        })
    }
}

impl<C: DiskConfig, A: ReplayApplication<C>> RaftSnapshotBuilder<C> for ReplayMachine<C, A> {
    async fn build_snapshot(&mut self) -> StoreResult<Snapshot<C>> {
        let mut state = self.state.lock().map_err(|e| failure(ErrorVerb::Read, e))?;
        state.meta.snapshot_id = state
            .meta
            .last_log_id
            .map_or_else(|| "empty".into(), |id| id.to_string());
        self.disk.write(|db| {
            let envelope = SnapshotEnvelope::from_image(&*state)?;
            save(db, "snapshot", &envelope)?;
            envelope.into_snapshot(state.meta.clone())
        })
    }
}
