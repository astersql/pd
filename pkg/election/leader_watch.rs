// Copyright 2026 AsterSQL.
use crate::{ElectionError, Result};
use astersql_pd_storage_kv::*;
use std::{collections::VecDeque, sync::Arc, time::Duration};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

pub(super) const CHECK_INTERVAL: Duration = Duration::from_secs(10);
const RETRY_INTERVAL: Duration = Duration::from_millis(100);
pub(super) const UNHEALTHY_TIMEOUT: Duration = Duration::from_secs(60);

/// Pull-based observation of one exact leadership key, inclusive from a revision.
/// No worker or private runtime is created. Drop releases the underlying MetaWatch.
/// Deletion and cancellation are terminal. Compaction is returned explicitly so
/// the caller can take a new snapshot before watching again (never skip history).
pub struct LeaderWatch {
    store: Arc<dyn MetaStore>,
    key: Vec<u8>,
    revision: Revision,
    pub(super) watch: Option<Box<dyn MetaWatch>>,
    pending: VecDeque<Event>,
    deleted: bool,
    failure: Option<StoreError>,
    unhealthy: Option<Instant>,
}
impl LeaderWatch {
    /// Observe through the existing metadata backend; no separate connection is opened.
    pub fn new(store: Arc<dyn MetaStore>, key: Vec<u8>, revision: Revision) -> Result<Self> {
        if key.is_empty() || revision <= 0 {
            return Err(ElectionError::InvalidArgument);
        }
        Ok(Self {
            store,
            key,
            revision,
            watch: None,
            pending: VecDeque::new(),
            deleted: false,
            failure: None,
            unhealthy: None,
        })
    }

    fn fail(&mut self, error: StoreError) -> ElectionError {
        self.watch = None;
        self.pending.clear();
        self.failure = Some(error.clone());
        if error == StoreError::Cancelled {
            ElectionError::Cancelled
        } else {
            error.into()
        }
    }

    /// Return the next exact-key event, retrying temporary backend disconnections.
    /// Successfully delivered batches resume at revision + 1 without replay.
    /// Healthy idle streams are periodically reopened; 60 seconds of backend
    /// unavailability terminates observation, matching Go's unhealthy exit bound.
    /// Dropping this future is safe; cancelling the token permanently closes it.
    pub async fn next(&mut self, cancel: &CancellationToken) -> Result<Event> {
        loop {
            if cancel.is_cancelled() {
                return Err(self.fail(StoreError::Cancelled));
            }
            if let Some(error) = self.failure.clone() {
                return Err(self.fail(error));
            }
            if self.deleted {
                return Err(ElectionError::NotLeader);
            }
            if let Some(event) = self.pending.pop_front() {
                if matches!(event, Event::Delete { .. }) {
                    self.deleted = true;
                    self.watch = None;
                    self.pending.clear();
                }
                return Ok(event);
            }
            if self
                .unhealthy
                .is_some_and(|start| start.elapsed() >= UNHEALTHY_TIMEOUT)
            {
                return Err(self.fail(StoreError::Unavailable));
            }
            // Bound both watch creation and receive, including a partition that
            // leaves an established stream silent. All IO remains cancellable.
            let deadline = self.unhealthy.map_or_else(
                || Instant::now() + CHECK_INTERVAL,
                |start| (start + UNHEALTHY_TIMEOUT).min(Instant::now() + CHECK_INTERVAL),
            );
            let operation = async {
                if self.watch.is_none() {
                    self.watch = Some(self.store.watch(self.key.clone(), self.revision).await?);
                }
                self.watch.as_mut().unwrap().next(cancel).await
            };
            let result = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(self.fail(StoreError::Cancelled)),
                result = tokio::time::timeout_at(deadline, operation) => result,
            };
            match result {
                Ok(Ok(batch)) => {
                    self.unhealthy = None;
                    self.pending
                        .extend(batch.events.into_iter().filter(|event| {
                            event.key() == self.key && event.revision() >= self.revision
                        }));
                    self.revision = batch.revision.saturating_add(1).max(self.revision);
                }
                Ok(Err(StoreError::Unavailable)) => {
                    self.unhealthy.get_or_insert_with(Instant::now);
                    // MetaWatch owns its reconnect cursor. Keep it on Unavailable.
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => return Err(self.fail(StoreError::Cancelled)),
                        _ = tokio::time::sleep(RETRY_INTERVAL) => {},
                    }
                }
                Ok(Err(error)) => return Err(self.fail(error)),
                Err(_) => {
                    self.watch = None;
                    // A silent watch is not proof of failure: distinguish a healthy
                    // idle key from an unavailable backend before starting the bound.
                    let health = tokio::select! {
                        biased;
                        _ = cancel.cancelled() => return Err(self.fail(StoreError::Cancelled)),
                        result = tokio::time::timeout_at(self.unhealthy.map_or_else(
                            || Instant::now() + CHECK_INTERVAL,
                            |start| (start + UNHEALTHY_TIMEOUT).min(Instant::now() + CHECK_INTERVAL),
                        ), self.store.read(
                            Read::range(&self.key, b"", Consistency::Linearizable)
                        )) => result,
                    };
                    match health {
                        Ok(Ok(_)) => self.unhealthy = None,
                        Ok(Err(StoreError::Unavailable)) | Err(_) => {
                            self.unhealthy.get_or_insert_with(Instant::now);
                        }
                        Ok(Err(error)) => return Err(self.fail(error)),
                    }
                }
            }
        }
    }
}
