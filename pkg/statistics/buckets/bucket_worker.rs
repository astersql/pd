// Copyright 2026 AsterSQL.

use std::{collections::HashMap, time::Instant};

use prometheus::{Histogram, HistogramOpts, HistogramVec, Registry, exponential_buckets};
use tokio::{
    runtime::Handle,
    sync::{mpsc, oneshot},
    task::{JoinError, JoinHandle},
};
use tokio_util::sync::CancellationToken;

use crate::{BucketRangeIndex, BucketReport, BucketStat, convert_to_bucket_tree_item};

/// Go's bounded bucket task queue capacity.
pub const BUCKET_TASK_QUEUE_CAPACITY: usize = 20_000;
/// An owned query snapshot, keyed by current region ID.
pub type BucketStats = HashMap<u64, Vec<BucketStat>>;

/// Nonblocking submission failure. Rejected tasks have no cache effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum BucketSubmitError {
    #[error("bucket task queue is full")]
    Full,
    #[error("bucket worker is closed")]
    Closed,
}

/// Query wait failure; cancellation never removes an accepted task from FIFO.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum BucketWaitError {
    #[error("bucket stats wait cancelled")]
    Cancelled,
    #[error("bucket worker stopped")]
    WorkerStopped,
}

enum Task {
    Check(BucketReport),
    Collect {
        min_degree: i32,
        region_ids: Vec<u64>,
        reply: oneshot::Sender<BucketStats>,
    },
}
impl Task {
    fn metric_index(&self) -> usize {
        match self {
            Self::Check(_) => 0,
            Self::Collect { .. } => 1,
        }
    }
}

/// Single-use, buffered response. Dropping or cancelling it cannot block execution.
pub struct BucketStatsWait {
    reply: oneshot::Receiver<BucketStats>,
    stopped: CancellationToken,
}
impl BucketStatsWait {
    /// Cancels the wait independently of the worker. Observed cancellation wins ties.
    pub async fn wait(
        self,
        cancellation: &CancellationToken,
    ) -> Result<BucketStats, BucketWaitError> {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(BucketWaitError::Cancelled),
            _ = self.stopped.cancelled() => Err(BucketWaitError::WorkerStopped),
            reply = self.reply => reply.map_err(|_| BucketWaitError::WorkerStopped),
        }
    }
}

/// Cloneable nonblocking producer; the worker exclusively owns all cache state.
#[derive(Clone)]
pub struct BucketWorkerClient {
    sender: mpsc::Sender<Task>,
    stopped: CancellationToken,
}
impl BucketWorkerClient {
    fn submit(&self, task: Task) -> Result<(), BucketSubmitError> {
        if self.stopped.is_cancelled() {
            return Err(BucketSubmitError::Closed);
        }
        self.sender.try_send(task).map_err(|err| match err {
            mpsc::error::TrySendError::Full(_) => BucketSubmitError::Full,
            mpsc::error::TrySendError::Closed(_) => BucketSubmitError::Closed,
        })
    }
    /// Accepts an owned report, preserving FIFO order with queries.
    pub fn check_async(&self, report: BucketReport) -> Result<(), BucketSubmitError> {
        self.submit(Task::Check(report))
    }
    /// Enqueues a query without waiting for capacity or its result.
    pub fn collect(
        &self,
        min_degree: i32,
        region_ids: Vec<u64>,
    ) -> Result<BucketStatsWait, BucketSubmitError> {
        let (reply, receiver) = oneshot::channel();
        self.submit(Task::Collect {
            min_degree,
            region_ids,
            reply,
        })?;
        Ok(BucketStatsWait {
            reply: receiver,
            stopped: self.stopped.clone(),
        })
    }
}

/// Host-runtime worker. Call shutdown to cancel and join before releasing resources.
pub struct BucketWorker {
    client: BucketWorkerClient,
    join: Option<JoinHandle<()>>,
}
impl BucketWorker {
    /// Moves the initial index into a host task and registers metrics in its registry.
    /// Duplicate metric registration fails before any background task is spawned.
    /// Parent cancellation stops this worker; shutdown never cancels the parent.
    pub fn start(
        handle: &Handle,
        parent: &CancellationToken,
        registry: &Registry,
        index: BucketRangeIndex,
    ) -> Result<Self, prometheus::Error> {
        let metrics = HistogramVec::new(
            HistogramOpts::new(
                "pd_scheduler_bucket_task_duration",
                "Bucketed histogram of processing time (s) of bucket task.",
            )
            .buckets(exponential_buckets(0.0001, 2.0, 23)?),
            &["type"],
        )?;
        registry.register(Box::new(metrics.clone()))?;
        let stopped = parent.child_token();
        let (sender, receiver) = mpsc::channel(BUCKET_TASK_QUEUE_CAPACITY);
        let join = handle.spawn(run(index, receiver, stopped.clone(), metrics));
        Ok(Self {
            client: BucketWorkerClient { sender, stopped },
            join: Some(join),
        })
    }
    #[cfg(test)]
    pub(crate) fn join_for_test(&mut self) -> JoinHandle<()> {
        self.join.take().unwrap()
    }

    /// Returns an independent producer handle.
    pub fn client(&self) -> BucketWorkerClient {
        self.client.clone()
    }
    /// Cancels pending work and waits for exit. Repeated calls are safe.
    /// Cancellation of this future retains the join handle for a later call.
    pub async fn shutdown(&mut self) -> Result<(), JoinError> {
        self.client.stopped.cancel();
        if let Some(join) = self.join.as_mut() {
            let result = join.await;
            self.join.take();
            result?;
        }
        Ok(())
    }
}
impl Drop for BucketWorker {
    fn drop(&mut self) {
        self.client.stopped.cancel();
    }
}

async fn run(
    mut index: BucketRangeIndex,
    mut receiver: mpsc::Receiver<Task>,
    stopped: CancellationToken,
    metrics: HistogramVec,
) {
    let _cancel_on_exit = stopped.clone().drop_guard();
    // Fixed labels are resolved once, outside the processing hot path.
    let task_metrics: [Histogram; 2] =
        ["check_buckets", "collect_bucket_stats"].map(|label| metrics.with_label_values(&[label]));
    loop {
        let task = tokio::select! {
            biased;
            _ = stopped.cancelled() => break,
            task = receiver.recv() => match task { Some(task) => task, None => break },
        };
        let started = Instant::now();
        let metric = &task_metrics[task.metric_index()];
        match task {
            Task::Check(report) => {
                let mut item = convert_to_bucket_tree_item(&report);
                let origins = match index.region(item.region_id) {
                    Some(old) if old.start_key == item.start_key && old.end_key == item.end_key => {
                        vec![old.clone()]
                    }
                    _ => index.get_buckets_by_key_range(&item.start_key, &item.end_key),
                };
                item.inherit(&origins);
                item.calculate_hot_degree();
                index.put_item(item);
            }
            Task::Collect {
                min_degree,
                region_ids,
                reply,
            } => {
                let _ = reply.send(index.get_hot_bucket_stats(min_degree, &region_ids));
            }
        }
        metric.observe(started.elapsed().as_secs_f64());
    }
}
