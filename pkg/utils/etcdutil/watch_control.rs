// Copyright 2026 AsterSQL.
use std::sync::{Arc, Mutex};

use astersql_pd_storage_kv::{MetaStore, Result, Revision, StoreError, WatchBatch};
use tokio::{
    sync::{Mutex as AsyncMutex, mpsc},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

use crate::{LoadedSnapshot, ReconciledWatch};

const RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// Shared control handle. Force requests are notifications, not completion receipts.
/// `wait_load` consumes the initial result once, just like Go's single-slot channel.
pub struct WatchControl {
    force: mpsc::Sender<()>,
    loaded: AsyncMutex<mpsc::Receiver<Result<()>>>,
    last_force: Mutex<Instant>,
}
impl WatchControl {
    pub fn force_load(&self) {
        self.force_load_at(Instant::now());
    }

    pub(crate) fn force_load_at(&self, now: Instant) {
        let mut last = self.last_force.lock().unwrap();
        if now.duration_since(*last) < RETRY_INTERVAL {
            return;
        }
        *last = now;
        let _ = self.force.try_send(());
    }

    /// A subsequent waiter blocks until its cancellation; forced loads never
    /// replenish this initial-result channel. Cancelling a waiter leaves the
    /// initial result available for another caller.
    pub async fn wait_load(&self, cancel: &CancellationToken) -> Result<()> {
        let mut receiver = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StoreError::Cancelled),
            receiver = self.loaded.lock() => receiver,
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(StoreError::Cancelled),
            result = receiver.recv() => result.unwrap_or(Err(StoreError::Unavailable)),
        }
    }
}

/// Host-driven loading and observation. The host calls `initialize` once, then
/// `apply_next` repeatedly, choosing retry timing after observation errors.
/// Full snapshots replace published state atomically; callbacks leave no effects
/// on error. No runtime, detached task or global cancellation is created here.
pub struct ControlledWatch {
    store: Arc<dyn MetaStore>,
    key: Vec<u8>,
    with_prefix: bool,
    batch_size: usize,
    force: mpsc::Receiver<()>,
    loaded: mpsc::Sender<Result<()>>,
    watch: Option<ReconciledWatch>,
    initial: Option<Result<()>>,
    cancelled: bool,
}
impl ControlledWatch {
    pub fn new(
        store: Arc<dyn MetaStore>,
        key: Vec<u8>,
        with_prefix: bool,
        batch_size: usize,
    ) -> (Arc<WatchControl>, Self) {
        let (force_tx, force) = mpsc::channel(1);
        let (loaded, loaded_rx) = mpsc::channel(1);
        (
            Arc::new(WatchControl {
                force: force_tx,
                loaded: AsyncMutex::new(loaded_rx),
                last_force: Mutex::new(Instant::now()),
            }),
            Self {
                store,
                key,
                with_prefix,
                batch_size,
                force,
                loaded,
                watch: None,
                initial: None,
                cancelled: false,
            },
        )
    }

    async fn load<F>(&mut self, cancel: &CancellationToken, publish: &mut F) -> Result<Revision>
    where
        F: FnMut(&LoadedSnapshot) -> Result<()>,
    {
        let (snapshot, watch) = ReconciledWatch::load(
            self.store.clone(),
            self.key.clone(),
            self.with_prefix,
            self.batch_size,
            cancel,
        )
        .await?;
        publish(&snapshot)?;
        let revision = watch.next_revision();
        self.watch = Some(watch);
        Ok(revision)
    }

    /// Three initial attempts, one second apart. Publication success precedes
    /// notification. Exhaustion returns the same final error to the initial waiter.
    /// Repeating initialization returns its saved outcome without publishing again.
    pub async fn initialize<F>(&mut self, cancel: &CancellationToken, mut publish: F) -> Result<()>
    where
        F: FnMut(&LoadedSnapshot) -> Result<()>,
    {
        if let Some(result) = &self.initial {
            return result.clone();
        }
        let mut result = Err(StoreError::Backend);
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::select! { biased; _ = cancel.cancelled() => { result = Err(StoreError::Cancelled); break; }, _ = tokio::time::sleep(RETRY_INTERVAL) => {} }
            }
            result = self.load(cancel, &mut publish).await.map(|_| ());
            if result.is_ok() || result == Err(StoreError::Cancelled) {
                break;
            }
        }
        self.cancelled = result == Err(StoreError::Cancelled);
        self.initial = Some(result.clone());
        let _ = self.loaded.try_send(result.clone());
        result
    }

    /// Await one live batch or forced complete replacement. Failed forced loads
    /// preserve the old observer. Neither forced outcome changes `wait_load`.
    /// Cancellation is terminal; the host must await its own worker before exit.
    pub async fn apply_next<F, G>(
        &mut self,
        cancel: &CancellationToken,
        mut publish: F,
        apply: G,
    ) -> Result<Revision>
    where
        F: FnMut(&LoadedSnapshot) -> Result<()>,
        G: FnMut(&WatchBatch) -> Result<()>,
    {
        if self.cancelled || cancel.is_cancelled() {
            self.cancelled = true;
            self.watch = None;
            return Err(StoreError::Cancelled);
        }
        if let Some(Err(error)) = &self.initial {
            return Err(error.clone());
        }
        let watch = self.watch.as_mut().ok_or(StoreError::InvalidArgument)?;
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(StoreError::Cancelled),
            Some(()) = self.force.recv() => self.load(cancel, &mut publish).await,
            result = watch.apply_next(cancel, apply) => result,
        };
        if result == Err(StoreError::Cancelled) {
            self.cancelled = true;
            self.watch = None;
        }
        result
    }
}
