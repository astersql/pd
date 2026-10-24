// Copyright 2026 AsterSQL.
//! Independently scheduled, bounded-memory downstream drains.
use crate::{HistoryBuffer, HistoryError, HistoryIndex, SyncFrame};
use astersql_pd_core::RegionInfo;
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tokio::{
    runtime::Handle,
    sync::{Notify, watch},
    task::JoinHandle,
};

/// Transport boundary. Dropping the send future must cancel its I/O; the sender
/// is owned exclusively by one worker, so no other task can call Send on it.
pub trait DownstreamSender: Send + 'static {
    /// Send a complete frame, returning success only after transport acceptance.
    fn send(&mut self, frame: SyncFrame) -> impl Future<Output = Result<(), String>> + Send;
}

/// A failed drain never acknowledges an unsent frame.
#[derive(Debug, thiserror::Error)]
pub enum DownstreamError {
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error("downstream send failed: {0}")]
    Send(String),
    #[error("downstream closed")]
    Closed,
}

struct Entry {
    index: AtomicU64,
    notify: Notify,
    keepalive: AtomicBool,
    cancel: watch::Sender<bool>,
}
struct State {
    history: HistoryBuffer,
    streams: BTreeMap<String, Arc<Entry>>,
}

/// History and binding boundary shared by all downstreams. No I/O runs under
/// its lock; notifications coalesce rather than queue copies of region records.
#[derive(Clone)]
pub struct Downstreams(Arc<Mutex<State>>);

/// Host-owned worker lifecycle. Call cancel then join during shutdown. Drop
/// requests cancellation as a fallback; join proves the sender was released.
pub struct DownstreamTask {
    entry: Arc<Entry>,
    join: Option<JoinHandle<Result<(), DownstreamError>>>,
}
impl DownstreamTask {
    pub(crate) async fn wait(
        &mut self,
    ) -> Result<Result<(), DownstreamError>, tokio::task::JoinError> {
        self.join.as_mut().expect("worker handle").await
    }
    /// Last successfully acknowledged exclusive history index.
    pub fn send_index(&self) -> HistoryIndex {
        HistoryIndex(self.entry.index.load(Ordering::Acquire))
    }
    /// Cancel waiting or in-flight I/O.
    pub fn cancel(&self) {
        self.entry.cancel.send_replace(true);
    }
    /// Wait for worker exit and sender destruction.
    pub async fn join(mut self) -> Result<Result<(), DownstreamError>, tokio::task::JoinError> {
        self.join.take().expect("worker handle").await
    }
}
impl Drop for DownstreamTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

// Removal is identity-checked: an old worker cannot remove its replacement.
struct Registration {
    owner: Downstreams,
    name: String,
    entry: Arc<Entry>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        let mut state = self.owner.0.lock().unwrap();
        if state
            .streams
            .get(&self.name)
            .is_some_and(|e| Arc::ptr_eq(e, &self.entry))
        {
            state.streams.remove(&self.name);
        }
    }
}
impl Downstreams {
    /// Own the live history; retained snapshots remain shared immutable values.
    pub fn new(history: HistoryBuffer) -> Self {
        Self(Arc::new(Mutex::new(State {
            history,
            streams: BTreeMap::new(),
        })))
    }
    /// Bind at an explicit successfully-sent index (e.g. full catch-up's result).
    /// The host supplies its runtime handle; the library creates no runtime.
    pub fn bind(
        &self,
        runtime: &Handle,
        name: String,
        start: HistoryIndex,
        mut sender: impl DownstreamSender,
    ) -> DownstreamTask {
        let (cancel, mut cancelled) = watch::channel(false);
        let entry = Arc::new(Entry {
            index: AtomicU64::new(start.0),
            notify: Notify::new(),
            keepalive: AtomicBool::new(false),
            cancel,
        });
        let registration = Registration {
            owner: self.clone(),
            name: name.clone(),
            entry: entry.clone(),
        };
        if let Some(old) = self.0.lock().unwrap().streams.insert(name, entry.clone()) {
            old.cancel.send_replace(true);
        }
        let worker_entry = entry.clone();
        let join = runtime.spawn(async move {
            let _registration = registration;
            loop {
                tokio::select! {
                    biased;
                    _ = cancelled.wait_for(|value| *value) => return Ok(()),
                    _ = worker_entry.notify.notified() => {}
                }
                let keepalive = worker_entry.keepalive.swap(false, Ordering::AcqRel);
                tokio::select! {
                    biased;
                    _ = cancelled.wait_for(|value| *value) => return Ok(()),
                    result = _registration.owner.drain(&_registration, &mut sender, keepalive) => result?,
                }
            }
        });
        DownstreamTask {
            entry,
            join: Some(join),
        }
    }
    /// Append only to history. Even a blocked Send never holds this lock.
    pub fn append(
        &self,
        records: impl IntoIterator<Item = Arc<RegionInfo>>,
    ) -> Result<(), HistoryError> {
        let mut state = self.0.lock().unwrap();
        for record in records {
            let next = state
                .history
                .next_index()
                .0
                .checked_add(1)
                .ok_or(HistoryError::IndexExhausted)?;
            if let Some(min) = state
                .streams
                .values()
                .map(|e| e.index.load(Ordering::Acquire))
                .min()
            {
                state
                    .history
                    .observe_required_window(next.saturating_sub(min))?;
            }
            state.history.record(record)?;
        }
        Ok(())
    }
    /// Wake every bound worker. Each drains history from its own acknowledged index.
    pub fn broadcast(&self, keepalive: bool) {
        for entry in self.0.lock().unwrap().streams.values() {
            if keepalive {
                entry.keepalive.store(true, Ordering::Release);
            }
            entry.notify.notify_one();
        }
    }
    pub(crate) fn attach_metrics(&self, metrics: crate::SyncMetrics) {
        self.0.lock().unwrap().history.attach_metrics(metrics);
    }
    /// Names of currently bound streams in lexical order.
    pub fn names(&self) -> Vec<String> {
        self.0.lock().unwrap().streams.keys().cloned().collect()
    }
    /// Snapshot retained history without waiting for any sender.
    pub fn records_from(&self, index: HistoryIndex) -> Vec<Arc<RegionInfo>> {
        self.0.lock().unwrap().history.records_from(index)
    }
    async fn drain(
        &self,
        registration: &Registration,
        sender: &mut impl DownstreamSender,
        keepalive: bool,
    ) -> Result<(), DownstreamError> {
        let mut sent = false;
        loop {
            let frame = {
                let state = self.0.lock().unwrap();
                if !state
                    .streams
                    .get(&registration.name)
                    .is_some_and(|entry| Arc::ptr_eq(entry, &registration.entry))
                {
                    return Err(DownstreamError::Closed);
                }
                let start = HistoryIndex(registration.entry.index.load(Ordering::Acquire));
                let first = state.history.first_index();
                let next = state.history.next_index();
                if start < first || start > next {
                    return Err(HistoryError::HistoryGap {
                        requested: start,
                        first,
                        next,
                    }
                    .into());
                }
                if start == next {
                    if !keepalive || sent {
                        return Ok(());
                    }
                    SyncFrame::keepalive(start)
                } else {
                    let end = HistoryIndex(
                        next.0.min(
                            start
                                .0
                                .saturating_add(crate::MAX_SYNC_REGION_BATCH_SIZE as u64),
                        ),
                    );
                    let records = state.history.records_between(start, end);
                    if records.len() as u64 != end.0 - start.0 {
                        return Err(HistoryError::HistoryGap {
                            requested: start,
                            first,
                            next,
                        }
                        .into());
                    }
                    SyncFrame::from_records(start, records)
                }
            };
            let end = frame.end_index();
            // No registry/history lock crosses this I/O boundary. Cancellation
            // drops the future; acknowledgement is synchronous after success.
            sender.send(frame).await.map_err(DownstreamError::Send)?;
            registration.entry.index.store(end.0, Ordering::Release);
            sent = true;
            // Give cancellation and other downstreams a turn even for ready I/O.
            tokio::task::yield_now().await;
        }
    }
}
