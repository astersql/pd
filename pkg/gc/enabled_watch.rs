// Copyright 2026 AsterSQL.
use crate::{EnabledKeyspace, EnabledSnapshot, EnabledSnapshotError, EnabledSnapshotLoader};
use astersql_pd_storage_domain::{Event, KeyValuePair, StoreError};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
#[derive(Default)]
struct IndexState {
    snapshot: Option<EnabledSnapshot>,
    stopped: bool,
    running: bool,
}
/// One host-owned leadership term's enabled metadata index. Run once, cancel and join at retirement.
pub struct EnabledKeyspaceIndex {
    loader: EnabledSnapshotLoader,
    state: Mutex<IndexState>,
    changed: tokio::sync::watch::Sender<u64>,
}
struct Retirement<'a>(&'a EnabledKeyspaceIndex);
impl Drop for Retirement<'_> {
    fn drop(&mut self) {
        self.0.state.lock().unwrap().stopped = true;
        self.0.signal();
    }
}
#[derive(Clone, Default)]
pub struct EnabledIndexHooks {
    pub on_page: Option<Arc<dyn Fn(Vec<EnabledKeyspace>) + Send + Sync>>,
    pub on_initial_snapshot: Option<Arc<dyn Fn(EnabledSnapshot) + Send + Sync>>,
}
impl EnabledKeyspaceIndex {
    pub fn new(loader: EnabledSnapshotLoader) -> Self {
        Self {
            loader,
            state: Mutex::new(IndexState::default()),
            changed: tokio::sync::watch::channel(0).0,
        }
    }
    fn signal(&self) {
        self.changed.send_modify(|v| *v = v.wrapping_add(1));
    }
    pub fn applied_revision(&self) -> i64 {
        self.state
            .lock()
            .unwrap()
            .snapshot
            .as_ref()
            .map_or(0, |s| s.revision)
    }
    /// Before the first complete snapshot, absence cannot establish obsolescence.
    pub fn can_skip_gc_state_warmup(&self, id: u32) -> bool {
        self.state
            .lock()
            .unwrap()
            .snapshot
            .as_ref()
            .is_some_and(|s| {
                s.entries
                    .get(&id)
                    .is_none_or(|e| e.gc_management_type != "keyspace_level")
            })
    }
    pub async fn run(&self, cancel: &CancellationToken) {
        self.run_with_hooks(cancel, EnabledIndexHooks::default())
            .await;
    }
    pub async fn run_with_hooks(&self, cancel: &CancellationToken, mut hooks: EnabledIndexHooks) {
        {
            let mut state = self.state.lock().unwrap();
            if state.running || state.stopped {
                return;
            }
            state.running = true;
        }
        let _retirement = Retirement(self);
        let mut retry = Duration::from_secs(1);
        while !cancel.is_cancelled() {
            let callback = hooks.on_page.clone().unwrap_or_else(|| Arc::new(|_| {}));
            let snapshot = self.loader.load_with_pages(cancel, callback).await;
            if let Ok(snapshot) = snapshot {
                let revision = snapshot.revision;
                {
                    let mut state = self.state.lock().unwrap();
                    if cancel.is_cancelled() {
                        break;
                    }
                    state.snapshot = Some(snapshot.clone());
                }
                self.signal();
                if let Some(callback) = hooks.on_initial_snapshot.take() {
                    callback(snapshot);
                }
                hooks.on_page = None;
                let started = Instant::now();
                let _error = self.watch(cancel, revision + 1).await;
                if started.elapsed() >= Duration::from_secs(30) {
                    retry = Duration::from_secs(1);
                }
            }
            tokio::select! {biased;_=cancel.cancelled()=>break,_=tokio::time::sleep(retry)=>{}}
            retry = (retry * 2).min(Duration::from_secs(30));
        }
    }
    pub async fn snapshot_at_least(
        &self,
        revision: i64,
        cancel: &CancellationToken,
    ) -> Result<EnabledSnapshot, EnabledSnapshotError> {
        let mut changed = self.changed.subscribe();
        loop {
            {
                let state = self.state.lock().unwrap();
                if state.stopped || cancel.is_cancelled() {
                    return Err(StoreError::Cancelled.into());
                }
                if let Some(s) = &state.snapshot
                    && s.revision >= revision
                {
                    return Ok(s.clone());
                }
            }
            tokio::select! {biased;_=cancel.cancelled()=>return Err(StoreError::Cancelled.into()),r=changed.changed()=>{if r.is_err(){return Err(StoreError::Cancelled.into());}}}
        }
    }
    async fn watch(
        &self,
        cancel: &CancellationToken,
        next: i64,
    ) -> Result<(), EnabledSnapshotError> {
        let storage = &self.loader.storage;
        let mut watch = tokio::select! {biased;_=cancel.cancelled()=>return Err(StoreError::Cancelled.into()),r=tokio::time::timeout(Duration::from_secs(5),storage.store.watch(storage.paths.keyspace_meta_prefix().into_bytes(),next))=>r.map_err(|_|StoreError::Unavailable)??};
        let mut pending: BTreeMap<u32, Option<EnabledKeyspace>> = BTreeMap::new();
        let mut published = next - 1;
        let mut pending_revision = published;
        let mut last_progress = Instant::now();
        loop {
            let batch = tokio::select! {biased;_=cancel.cancelled()=>return Err(StoreError::Cancelled.into()),r=tokio::time::timeout(Duration::from_secs(10).saturating_sub(last_progress.elapsed()),watch.next_with_progress(cancel))=>r.map_err(|_|StoreError::Unavailable)??};
            if batch.events.is_empty() {
                if batch.revision < pending_revision
                    || batch.revision < published
                    || (batch.revision == published && !pending.is_empty())
                {
                    return Err(EnabledSnapshotError::Invalid(
                        "keyspace metadata progress precedes applied events".into(),
                    ));
                }
                {
                    let mut state = self.state.lock().unwrap();
                    if cancel.is_cancelled() {
                        return Err(StoreError::Cancelled.into());
                    }
                    if let Some(snapshot) = &mut state.snapshot
                        && batch.revision > snapshot.revision
                    {
                        for (id, entry) in std::mem::take(&mut pending) {
                            if let Some(entry) = entry {
                                snapshot.entries.insert(id, entry);
                            } else {
                                snapshot.entries.remove(&id);
                            }
                        }
                        snapshot.revision = batch.revision;
                    }
                }
                self.signal();
                published = batch.revision;
                pending_revision = published;
                last_progress = Instant::now();
                continue;
            }
            let mut decoded = Vec::with_capacity(batch.events.len());
            for event in &batch.events {
                if event.revision() <= published {
                    return Err(EnabledSnapshotError::Invalid(
                        "stale keyspace metadata event".into(),
                    ));
                }
                pending_revision = pending_revision.max(event.revision());
                match event {
                    Event::Put(entry) => {
                        let pair = KeyValuePair {
                            key: entry.key.clone(),
                            value: entry.value.clone(),
                        };
                        let (entries, id) =
                            self.loader.decode_page(&[pair], false, event.revision())?;
                        decoded.push((id, entries.into_iter().next()));
                    }
                    Event::Delete { key, .. } => {
                        let prefix = storage.paths.keyspace_meta_prefix();
                        let raw = std::str::from_utf8(key)
                            .map_err(|e| EnabledSnapshotError::Invalid(e.to_string()))?;
                        let id = raw
                            .strip_prefix(&prefix)
                            .and_then(|s| s.parse::<u32>().ok())
                            .ok_or_else(|| {
                                EnabledSnapshotError::Invalid(format!(
                                    "invalid deleted keyspace metadata: {raw:?}"
                                ))
                            })?;
                        decoded.push((id, None));
                    }
                }
            }
            for (id, entry) in decoded {
                pending.insert(id, entry);
            }
        }
    }
}
