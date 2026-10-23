// Copyright 2026 AsterSQL.
//! In-memory contract model, not a durable production backend. No worker tasks.
use crate::meta_store::validate_txn;
use crate::*;
use async_trait::async_trait;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::Notify, time::Instant};
use tokio_util::sync::CancellationToken;

type Keys = BTreeMap<Vec<u8>, Entry>;
struct State {
    revision: Revision,
    compacted: Revision,
    keys: Keys,
    snapshots: BTreeMap<Revision, Keys>,
    events: Vec<Event>,
    leases: BTreeMap<LeaseId, (i64, Instant)>,
    next_lease: LeaseId,
}
impl Default for State {
    fn default() -> Self {
        Self {
            revision: 1,
            compacted: 0,
            keys: Keys::new(),
            snapshots: BTreeMap::from([(1, Keys::new())]),
            events: Vec::new(),
            leases: BTreeMap::new(),
            next_lease: 1,
        }
    }
}
impl State {
    fn save(&mut self) {
        self.snapshots.insert(self.revision, self.keys.clone());
    }
    fn remove_lease(&mut self, id: LeaseId) {
        self.leases.remove(&id);
        let keys: Vec<_> = self
            .keys
            .values()
            .filter(|e| e.lease == Some(id))
            .map(|e| e.key.clone())
            .collect();
        if !keys.is_empty() {
            self.revision += 1;
            for key in keys {
                self.keys.remove(&key);
                self.events.push(Event::Delete {
                    key,
                    revision: self.revision,
                });
            }
            self.save();
        }
    }
    fn expire(&mut self) {
        let expired: Vec<_> = self
            .leases
            .iter()
            .filter(|(_, (_, end))| *end <= Instant::now())
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            self.remove_lease(id);
        }
    }
}
#[derive(Default, Clone)]
pub struct MemoryStore {
    state: Arc<Mutex<State>>,
    changed: Arc<Notify>,
}
impl MemoryStore {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        let mut s = self.state.lock().expect("memory model mutex poisoned");
        let rev = s.revision;
        s.expire();
        if s.revision != rev {
            self.changed.notify_waiters();
        }
        s
    }
}
#[async_trait]
impl MetaStore for MemoryStore {
    async fn read(&self, r: Read) -> Result<Snapshot> {
        r.validate()?;
        let s = self.state();
        let rev = r.revision.unwrap_or(s.revision);
        if rev < s.compacted {
            return Err(StoreError::Compacted {
                requested: rev,
                compact_revision: Some(s.compacted),
            });
        }
        if rev > s.revision {
            return Err(StoreError::FutureRevision);
        }
        let keys = s
            .snapshots
            .range(..=rev)
            .next_back()
            .ok_or(StoreError::Backend)?
            .1;
        let start = r.start.as_ref().unwrap_or(&r.prefix);
        let mut selected = keys
            .range(start.clone()..)
            .map(|(_, entry)| entry)
            .take_while(|e| match &r.end {
                Some(end) if end.is_empty() => e.key == *start,
                Some(end) => e.key < *end,
                None => e.key.starts_with(&r.prefix),
            });
        let entries = if r.limit == 0 {
            selected.by_ref().cloned().collect()
        } else {
            selected.by_ref().take(r.limit).cloned().collect()
        };
        let more = selected.next().is_some();
        Ok(Snapshot {
            revision: s.revision,
            entries,
            more,
        })
    }
    async fn transact(&self, compares: Vec<Compare>, writes: Vec<Mutation>) -> Result<Commit> {
        self.transact_branches(compares, writes, vec![]).await
    }
    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        then_writes: Vec<Mutation>,
        else_writes: Vec<Mutation>,
    ) -> Result<Commit> {
        validate_txn(&compares, &then_writes)?;
        validate_txn(&compares, &else_writes)?;
        let mut s = self.state();
        let succeeded = compares.iter().all(|c| match c {
            Compare::CreateRevision(k, rev) => {
                s.keys.get(k).map_or(0, |e| e.create_revision) == *rev
            }
            Compare::ModRevision(k, rev) => s.keys.get(k).map_or(0, |e| e.mod_revision) == *rev,
            Compare::Value(k, value) => s.keys.get(k).is_some_and(|e| e.value == *value),
            Compare::ValueNotEqual(k, value) => s.keys.get(k).is_some_and(|e| e.value != *value),
            Compare::ValueLess(k, value) => s.keys.get(k).is_some_and(|e| e.value < *value),
            Compare::ValueGreater(k, value) => s.keys.get(k).is_some_and(|e| e.value > *value),
            Compare::Exists(k) => s.keys.contains_key(k),
            Compare::NotExists(k) => !s.keys.contains_key(k),
            Compare::Lease(k, id) => s.keys.get(k).and_then(|e| e.lease).unwrap_or(0) == *id,
        });
        let writes = if succeeded { then_writes } else { else_writes };
        for w in &writes {
            if let Mutation::Put {
                lease: Some(id), ..
            } = w
            {
                if !s.leases.contains_key(id) {
                    return Err(StoreError::LeaseNotFound);
                }
            }
        }
        let changes = writes
            .iter()
            .any(|w| matches!(w, Mutation::Put { .. }) || s.keys.contains_key(w.key()));
        if changes {
            s.revision += 1;
            for w in writes {
                match w {
                    Mutation::Put { key, value, lease } => {
                        let create_revision =
                            s.keys.get(&key).map_or(s.revision, |e| e.create_revision);
                        let entry = Entry {
                            key: key.clone(),
                            value,
                            lease,
                            create_revision,
                            mod_revision: s.revision,
                        };
                        s.keys.insert(key, entry.clone());
                        s.events.push(Event::Put(entry));
                    }
                    Mutation::Delete { key } => {
                        if s.keys.remove(&key).is_some() {
                            let revision = s.revision;
                            s.events.push(Event::Delete { key, revision });
                        }
                    }
                }
            }
            s.save();
            self.changed.notify_waiters();
        }
        Ok(Commit {
            succeeded,
            revision: s.revision,
        })
    }
    async fn watch(&self, prefix: Vec<u8>, start_revision: Revision) -> Result<Box<dyn MetaWatch>> {
        if start_revision <= 0 {
            return Err(StoreError::InvalidArgument);
        }
        Ok(Box::new(MemoryWatch {
            store: self.clone(),
            prefix,
            next: start_revision,
            cancelled: false,
        }))
    }
    async fn grant(&self, ttl_seconds: i64) -> Result<Lease> {
        // Bound TTL to avoid Instant overflow, matching the adapter's accepted range.
        if !(1..=i32::MAX as i64).contains(&ttl_seconds) {
            return Err(StoreError::InvalidArgument);
        }
        let mut s = self.state();
        let id = s.next_lease;
        s.next_lease += 1;
        s.leases.insert(
            id,
            (
                ttl_seconds,
                Instant::now() + Duration::from_secs(ttl_seconds as u64),
            ),
        );
        Ok(Lease { id, ttl_seconds })
    }
    async fn keep_alive(&self, id: LeaseId) -> Result<Lease> {
        let mut s = self.state();
        let (ttl, end) = s.leases.get_mut(&id).ok_or(StoreError::LeaseNotFound)?;
        *end = Instant::now() + Duration::from_secs(*ttl as u64);
        Ok(Lease {
            id,
            ttl_seconds: *ttl,
        })
    }
    async fn revoke(&self, id: LeaseId) -> Result<()> {
        let mut s = self.state();
        if !s.leases.contains_key(&id) {
            return Err(StoreError::LeaseNotFound);
        }
        s.remove_lease(id);
        self.changed.notify_waiters();
        Ok(())
    }
    async fn ttl(&self, id: LeaseId) -> Result<i64> {
        let s = self.state();
        let (_, end) = s.leases.get(&id).ok_or(StoreError::LeaseNotFound)?;
        Ok(end.saturating_duration_since(Instant::now()).as_secs() as i64)
    }
    async fn compact(&self, rev: Revision) -> Result<()> {
        let mut s = self.state();
        if rev <= 0 {
            return Err(StoreError::InvalidArgument);
        }
        if rev <= s.compacted {
            return Err(StoreError::Compacted {
                requested: rev,
                compact_revision: Some(s.compacted),
            });
        }
        if rev > s.revision {
            return Err(StoreError::FutureRevision);
        }
        s.compacted = rev;
        s.events.retain(|e| e.revision() > rev);
        s.snapshots.retain(|r, _| *r >= rev);
        self.changed.notify_waiters();
        Ok(())
    }
}
struct MemoryWatch {
    store: MemoryStore,
    prefix: Vec<u8>,
    next: Revision,
    cancelled: bool,
}
#[async_trait]
impl MetaWatch for MemoryWatch {
    async fn next(&mut self, cancel: &CancellationToken) -> Result<WatchBatch> {
        loop {
            if self.cancelled || cancel.is_cancelled() {
                self.cancelled = true;
                return Err(StoreError::Cancelled);
            }
            let notified = self.store.changed.notified();
            let deadline = {
                let s = self.store.state();
                if self.next <= s.compacted {
                    return Err(StoreError::Compacted {
                        requested: self.next,
                        compact_revision: Some(s.compacted),
                    });
                }
                let events: Vec<_> = s
                    .events
                    .iter()
                    .filter(|e| e.revision() >= self.next && e.key().starts_with(&self.prefix))
                    .cloned()
                    .collect();
                if let Some(last) = events.last() {
                    let revision = last.revision();
                    self.next = revision + 1;
                    return Ok(WatchBatch { events, revision });
                }
                s.leases
                    .values()
                    .map(|(_, end)| *end)
                    .min()
                    .unwrap_or(Instant::now() + Duration::from_secs(86400))
            };
            tokio::select! {
                biased;
                _ = cancel.cancelled() => { self.cancelled = true; return Err(StoreError::Cancelled); },
                _ = notified => {},
                _ = tokio::time::sleep_until(deadline) => {},
            }
        }
    }
}
