// Copyright 2026 AsterSQL.
//! Experimental state machine derived from durable snapshots and quorum logs.

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
    pub(super) async fn wait_for_snapshot(
        &self,
        disk: &RaftDisk,
        log_id: LogId<u64>,
    ) -> StoreResult<()> {
        let wait = async {
            loop {
                let mut changed = std::pin::pin!(self.changed.notified());
                changed.as_mut().enable();
                let receiving = self.receiving.load(Ordering::Acquire);
                let covered = disk.read(|db| {
                    Ok(load::<SnapshotEnvelope>(db, "snapshot")?
                        .map(|snapshot| snapshot.image())
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

#[derive(Default)]
struct State {
    applied: Option<LogId<u64>>,
    membership: Membership,
    records: BTreeMap<Vec<u8>, Vec<u8>>,
    revision: u64,
}

impl From<Image> for State {
    fn from(image: Image) -> Self {
        Self {
            applied: image.meta.last_log_id,
            membership: image.meta.last_membership,
            records: image.records.into_iter().collect(),
            revision: image.revision,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Replay {
    disk: RaftDisk,
    state: Arc<Mutex<State>>,
}

impl Replay {
    pub(crate) fn open(mut disk: RaftDisk) -> StoreResult<(RaftDisk, Self)> {
        let state = disk.read(|db| {
            if load::<LogId<u64>>(db, "applied")?.is_some() {
                return Err(io::Error::other("replay requires a replay-format database").into());
            }
            let snapshot = load::<SnapshotEnvelope>(db, "snapshot")?
                .map(|snapshot| snapshot.image())
                .transpose()?;
            Ok(snapshot.map(State::from).unwrap_or_default())
        })?;
        disk.1 = Some(Arc::new(InstallFence::default()));
        Ok((
            disk.clone(),
            Self {
                disk,
                state: Arc::new(Mutex::new(state)),
            },
        ))
    }

    // Callers must first establish the leader's read barrier. A reopened local
    // snapshot can be older than the latest acknowledged transaction.
    pub(crate) fn value(&self, key: &[u8]) -> StoreResult<Option<Vec<u8>>> {
        let state = self.state.lock().map_err(|e| failure(ErrorVerb::Read, e))?;
        Ok(state.records.get(key).cloned())
    }

    pub(crate) fn read_pair(&self, keys: [&[u8]; 2]) -> StoreResult<(u64, [Option<Vec<u8>>; 2])> {
        let state = self.state.lock().map_err(|e| failure(ErrorVerb::Read, e))?;
        Ok((
            state.revision,
            keys.map(|key| state.records.get(key).cloned()),
        ))
    }
}

impl RaftStateMachine<Type> for Replay {
    type SnapshotBuilder = Self;

    async fn applied_state(&mut self) -> StoreResult<(Option<LogId<u64>>, Membership)> {
        let state = self.state.lock().map_err(|e| failure(ErrorVerb::Read, e))?;
        Ok((state.applied, state.membership.clone()))
    }

    async fn apply<I>(&mut self, entries: I) -> StoreResult<Vec<u64>>
    where
        I: IntoIterator<Item = Entry<Type>> + Send,
        I::IntoIter: Send,
    {
        let started = Instant::now();
        let entries: Vec<_> = entries.into_iter().collect();
        let mut state = self
            .state
            .lock()
            .map_err(|e| failure(ErrorVerb::Write, e))?;
        let mut previous = state.applied;
        let mut last_revision = state.revision;
        for entry in &entries {
            if previous.map_or(Some(0), |id| id.index.checked_add(1)) != Some(entry.log_id.index) {
                return Err(failure(ErrorVerb::Write, "non-contiguous replay log"));
            }
            if matches!(entry.payload, EntryPayload::Normal(_)) {
                last_revision = last_revision
                    .checked_add(1)
                    .ok_or_else(|| failure(ErrorVerb::Write, "revision exhausted"))?;
            }
            previous = Some(entry.log_id);
        }

        let mut replies = Vec::with_capacity(entries.len());
        for entry in entries {
            match entry.payload {
                EntryPayload::Blank => {}
                EntryPayload::Membership(membership) => {
                    state.membership = Membership::new(Some(entry.log_id), membership)
                }
                EntryPayload::Normal(PutPair(records)) => {
                    state.records.extend(records);
                    state.revision += 1;
                }
            }
            state.applied = Some(entry.log_id);
            replies.push(state.revision);
        }
        event("apply", previous.map_or(0, |id| id.index), started);
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
            let image = envelope.image().map_err(|e| failure(ErrorVerb::Read, e))?;
            if &image.meta != meta {
                return Err(failure(ErrorVerb::Read, "snapshot metadata mismatch"));
            }
            let mut state = self
                .state
                .lock()
                .map_err(|e| failure(ErrorVerb::Write, e))?;
            if state.applied > image.meta.last_log_id {
                return Err(failure(
                    ErrorVerb::Write,
                    "snapshot would roll back applied state",
                ));
            }
            self.disk.write(|db| save(db, "snapshot", &envelope))?;
            *state = image.into();
            Ok(())
        })();
        self.disk.1.as_ref().unwrap().complete();
        result
    }

    async fn get_current_snapshot(&mut self) -> StoreResult<Option<Snapshot<Type>>> {
        self.disk.get_current_snapshot().await
    }
}

impl RaftSnapshotBuilder<Type> for Replay {
    async fn build_snapshot(&mut self) -> StoreResult<Snapshot<Type>> {
        let state = self.state.lock().map_err(|e| failure(ErrorVerb::Read, e))?;
        let meta = Meta {
            snapshot_id: state
                .applied
                .map_or_else(|| "empty".into(), |id| id.to_string()),
            last_log_id: state.applied,
            last_membership: state.membership.clone(),
        };
        let image = Image {
            meta: meta.clone(),
            records: state
                .records
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            revision: state.revision,
        };
        self.disk.write(|db| {
            let envelope = SnapshotEnvelope::from_image(&image)?;
            save(db, "snapshot", &envelope)?;
            envelope.into_snapshot(meta)
        })
    }
}
