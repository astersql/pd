// Copyright 2026 AsterSQL.
use crate::{ElectionError, Leadership};
use std::sync::Arc;
use tokio::{runtime::Handle, task::JoinHandle};
use tokio::{
    task::JoinSet,
    time::{Instant, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;

/// Reason a caller-driven renewal loop stopped.
#[derive(Debug, PartialEq, Eq)]
pub enum LeaseKeepExit {
    Cancelled,
    TimedOut,
    NotLeader,
}
/// Owned cancellation and join handle for a renewal loop on the host runtime.
pub struct LeaseKeep {
    cancel: CancellationToken,
    join: Option<JoinHandle<LeaseKeepExit>>,
}
impl LeaseKeep {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    pub fn is_finished(&self) -> bool {
        self.join.as_ref().unwrap().is_finished()
    }
    pub async fn join(mut self) -> std::result::Result<LeaseKeepExit, tokio::task::JoinError> {
        self.join.take().unwrap().await
    }
}
impl Drop for LeaseKeep {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
impl Leadership {
    /// Start renewal on the supplied runtime; dropping the handle requests cancellation.
    pub fn keep(self: &Arc<Self>, runtime: &Handle, cancel: CancellationToken) -> LeaseKeep {
        let cancel = cancel.child_token();
        let term = self.clone();
        let worker_cancel = cancel.clone();
        let join = runtime.spawn(async move { term.run_keep(worker_cancel).await });
        LeaseKeep {
            cancel,
            join: Some(join),
        }
    }
}

impl Leadership {
    async fn run_keep(self: Arc<Self>, cancel: CancellationToken) -> LeaseKeepExit {
        // Zero cannot occur after a valid grant, but do not panic on a custom backend.
        if self.timeout.is_zero() {
            return LeaseKeepExit::TimedOut;
        }
        let mut ticker = tokio::time::interval(self.timeout / 3);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let timer = tokio::time::sleep(self.timeout);
        tokio::pin!(timer);
        let mut requests = JoinSet::new();
        let exit = loop {
            tokio::select! {
                biased;
                _ = self.keep_cancel.cancelled() => break LeaseKeepExit::Cancelled,
                _ = cancel.cancelled() => break LeaseKeepExit::Cancelled,
                _ = &mut timer => break LeaseKeepExit::TimedOut,
                response = requests.join_next(), if !requests.is_empty() => {
                    match response {
                        Some(Ok(Ok((start, ttl)))) if ttl > 0 => {
                            // Recheck cancellation before publishing a late response.
                            if self.keep_cancel.is_cancelled() || cancel.is_cancelled() {
                                break LeaseKeepExit::Cancelled;
                            }
                            self.expiry.renewed(start, ttl);
                            timer.as_mut().reset(Instant::now() + self.timeout);
                        }
                        Some(Ok(Err(ElectionError::NotLeader))) => break LeaseKeepExit::NotLeader,
                        _ => {} // IO errors and invalid TTL do not reset the success timer.
                    }
                }
                _ = ticker.tick() => {
                    let term = self.clone();
                    let start = term.expiry.request_start();
                    requests.spawn(async move {
                        tokio::time::timeout(term.timeout, async {
                            term.validate().await?;
                            let lease = term.store.keep_alive(term.lease).await
                                .map_err(|e| match e {
                                    astersql_pd_storage_domain::StoreError::LeaseNotFound => ElectionError::NotLeader,
                                    other => ElectionError::Store(other),
                                })?;
                            term.validate().await?;
                            Ok((start, lease.ttl_seconds))
                        }).await.unwrap_or(Err(ElectionError::Store(astersql_pd_storage_domain::StoreError::Unavailable)))
                    });
                }
            }
        };
        // Cancel and drain every in-flight request before reporting join completion.
        requests.abort_all();
        while requests.join_next().await.is_some() {}
        exit
    }
}
