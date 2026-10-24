// Copyright 2026 AsterSQL.
//! Host-owned cancellation, retry and join boundary for region synchronization.
use crate::{
    DownstreamError, DownstreamSender, DownstreamTask, Downstreams, HistoryIndex, SyncFrame,
};
use prometheus::{Gauge, GaugeVec, Opts, Registry};
use std::{future::Future, time::Duration};
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// Outcome of an entire connect/request/receive attempt. EOF or leader loss ends
/// the worker; transient errors and Retry wait before opening another stream.
pub enum SyncAttemptOutcome {
    Complete,
    Retry,
}
/// Transport boundary including initial request and receive. Dropping its future
/// must release I/O and stream resources; attempts may not spawn detached tasks.
pub trait SyncAttempt: Send + 'static {
    fn sync(&mut self) -> impl Future<Output = Result<SyncAttemptOutcome, String>> + Send;
}

/// Existing Go history gauges, registered only with the host's registry.
#[derive(Clone, Debug)]
pub struct SyncMetrics {
    sync: Gauge,
    first: Gauge,
    last: Gauge,
}
impl SyncMetrics {
    /// Register once per syncer/registry; duplicate registration is an error.
    pub fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let gauges = GaugeVec::new(
            Opts::new(
                "pd_region_syncer_status",
                "Inner status of the region syncer.",
            ),
            &["type"],
        )?;
        let metrics = Self {
            sync: gauges.with_label_values(&["sync_index"]),
            first: gauges.with_label_values(&["first_index"]),
            last: gauges.with_label_values(&["last_index"]),
        };
        registry.register(Box::new(gauges))?;
        Ok(metrics)
    }
    pub(crate) fn record_metrics(&self, assigned: HistoryIndex) {
        self.sync.set(assigned.0 as f64);
    }
    pub(crate) fn persist_metrics(&self, first: HistoryIndex, next: HistoryIndex) {
        self.first.set(first.0 as f64);
        self.last.set(next.0 as f64);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SyncLifecycleError {
    #[error("region syncer stopped")]
    Closed,
    #[error(transparent)]
    Downstream(#[from] DownstreamError),
    #[error("region syncer worker failed: {0}")]
    Worker(#[from] tokio::task::JoinError),
}

/// One start/stop generation. The host provides its runtime and calls stop to
/// cancel every worker before joining any of them. Drop requests cancellation;
/// only stop establishes that all resources have actually been reclaimed.
pub struct SyncLifecycle {
    runtime: Handle,
    cancel: CancellationToken,
    downstreams: Downstreams,
    tasks: Vec<DownstreamTask>,
    clients: Vec<JoinHandle<()>>,
}
impl SyncLifecycle {
    pub fn new(runtime: Handle, downstreams: Downstreams, metrics: SyncMetrics) -> Self {
        downstreams.attach_metrics(metrics);
        Self {
            runtime,
            cancel: CancellationToken::new(),
            downstreams,
            tasks: Vec::new(),
            clients: Vec::new(),
        }
    }
    /// Independent host cancellation may precede the first request.
    pub fn cancel(&self) {
        self.cancel.cancel();
        for task in &self.tasks {
            task.cancel();
        }
    }
    pub fn start_downstream(
        &mut self,
        name: String,
        start: HistoryIndex,
        sender: impl DownstreamSender,
        send_timeout: Duration,
    ) -> Result<(), SyncLifecycleError> {
        if self.cancel.is_cancelled() {
            return Err(SyncLifecycleError::Closed);
        }
        self.tasks.push(self.downstreams.bind(
            &self.runtime,
            name,
            start,
            DeadlineSender {
                sender,
                timeout: if send_timeout.is_zero() {
                    Duration::from_secs(10)
                } else {
                    send_timeout
                },
            },
        ));
        Ok(())
    }
    /// Start a serialized follower attempt/retry loop on the supplied runtime.
    pub fn start_client(
        &mut self,
        mut attempt: impl SyncAttempt,
        retry_interval: Duration,
    ) -> Result<(), SyncLifecycleError> {
        if self.cancel.is_cancelled() {
            return Err(SyncLifecycleError::Closed);
        }
        let cancel = self.cancel.child_token();
        self.clients.push(self.runtime.spawn(async move {
            loop {
                let outcome = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return,
                    result = attempt.sync() => result,
                };
                if matches!(outcome, Ok(SyncAttemptOutcome::Complete)) {
                    return;
                }
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(retry_interval) => {},
                }
            }
        }));
        Ok(())
    }
    /// Idempotent shutdown; collect errors only after joining ALL workers. The
    /// vectors retain pending handles if the host temporarily drops this future.
    pub async fn stop(&mut self) -> Vec<SyncLifecycleError> {
        self.cancel();
        let mut errors = Vec::new();
        while let Some(client) = self.clients.last_mut() {
            if let Err(error) = client.await {
                errors.push(error.into());
            }
            self.clients.pop();
        }
        while let Some(task) = self.tasks.last_mut() {
            match task.wait().await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => errors.push(error.into()),
                Err(error) => errors.push(error.into()),
            }
            self.tasks.pop();
        }
        errors
    }
}
impl Drop for SyncLifecycle {
    fn drop(&mut self) {
        self.cancel();
    }
}

// One worker exclusively owns this adapter and its transport. Timeout drops the
// I/O future before returning an error; no second Send or detached timer task.
struct DeadlineSender<S> {
    sender: S,
    timeout: Duration,
}
impl<S: DownstreamSender> DownstreamSender for DeadlineSender<S> {
    async fn send(&mut self, frame: SyncFrame) -> Result<(), String> {
        tokio::time::timeout(self.timeout, self.sender.send(frame))
            .await
            .map_err(|_| "region syncer send timeout".to_owned())?
    }
}
