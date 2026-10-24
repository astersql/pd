// Copyright 2026 AsterSQL.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
use astersql_pd_statistics_utils::RWType;
use tokio::{
    runtime::Handle,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

use crate::{HotPeerCache, HotPeerStat};

/// Immutable directional commands. Region metadata and load arrays are owned.
#[derive(Clone, Debug)]
pub enum HotTask {
    Expire {
        meta: RegionMeta,
    },
    Write {
        meta: RegionMeta,
        leader_store: u64,
        interval: u64,
        counters: [u64; 3],
    },
    Read {
        meta: RegionMeta,
        leader_store: u64,
        store: u64,
        loads: [f64; 8],
        interval: u64,
    },
    Cold {
        store: u64,
        reported: BTreeSet<u64>,
        interval: u64,
    },
    Stats {
        min_degree: i64,
    },
    Peer {
        region: u64,
        store: u64,
    },
    IsHot {
        meta: RegionMeta,
        min_degree: i64,
    },
}

/// Captures the Go compact expiration and write tasks without retaining RegionInfo.
pub fn new_region_flow_tasks(region: &RegionInfo) -> (HotTask, HotTask) {
    let (start, end) = region.report_interval();
    (
        HotTask::Expire {
            meta: region.meta().clone(),
        },
        HotTask::Write {
            meta: region.meta().clone(),
            leader_store: region.leader().map_or(0, |p| p.store_id),
            interval: end.wrapping_sub(start),
            counters: [
                region.stats().written_bytes,
                region.stats().written_keys,
                region.write_query_num(),
            ],
        },
    )
}

impl HotTask {
    /// Captures a store-local read report and immutable membership.
    pub fn read_peer(region: &RegionInfo, store: u64, loads: [f64; 8], interval: u64) -> Self {
        Self::Read {
            meta: region.meta().clone(),
            leader_store: region.leader().map_or(0, |p| p.store_id),
            store,
            loads,
            interval,
        }
    }
}

/// Admission and execution errors never imply that a report was applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotTaskError {
    Full,
    Closed,
    Canceled,
    WorkerStopped,
    WrongDirection,
}
impl std::fmt::Display for HotTaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "hot task {self:?}")
    }
}
impl std::error::Error for HotTaskError {}

/// Completed command result; snapshots own their data and contain no live windows.
#[derive(Debug)]
pub enum HotTaskOutput {
    Applied,
    Stats(BTreeMap<u64, Vec<HotPeerStat>>),
    Peer(Option<HotPeerStat>),
    IsHot(bool),
}

impl HotPeerCache {
    /// Executes the same command used by the asynchronous worker, synchronously.
    /// Host-clock nanoseconds are captured at admission, not read from a global clock.
    pub fn run_task(&mut self, task: HotTask, now: i64) -> Result<HotTaskOutput, HotTaskError> {
        let actions = match task {
            HotTask::Expire { meta } => self.collect_expired_items(&task_region(meta, 0)),
            HotTask::Write {
                meta,
                leader_store,
                interval,
                counters,
            } => {
                if self.kind != RWType::WRITE {
                    return Err(HotTaskError::WrongDirection);
                }
                let mut loads = [0.; 8];
                loads[3] = counters[0] as f64;
                loads[4] = counters[1] as f64;
                loads[5] = counters[2] as f64;
                self.check_peer_flow_dynamic(
                    &task_region(meta, leader_store),
                    None,
                    &loads,
                    interval,
                    now,
                )
            }
            HotTask::Read {
                meta,
                leader_store,
                store,
                loads,
                interval,
            } => {
                if self.kind != RWType::READ {
                    return Err(HotTaskError::WrongDirection);
                }
                self.check_peer_flow_dynamic(
                    &task_region(meta, leader_store),
                    Some(&[store]),
                    &loads,
                    interval,
                    now,
                )
            }
            HotTask::Cold {
                store,
                reported,
                interval,
            } => {
                if self.kind != RWType::READ {
                    return Err(HotTaskError::WrongDirection);
                }
                self.check_cold_peer(store, &reported, interval, now)
            }
            HotTask::Stats { min_degree } => {
                let mut stats = BTreeMap::<u64, Vec<HotPeerStat>>::new();
                for peer in self.peers.values() {
                    let store = stats.entry(peer.store_id).or_default();
                    if peer.hot_degree >= min_degree
                        && !peer.in_cold
                        && peer.anti_count == self.kind.default_anti_count() as i64
                    {
                        store.push(peer.clone());
                    }
                }
                return Ok(HotTaskOutput::Stats(stats));
            }
            HotTask::Peer { region, store } => {
                return Ok(HotTaskOutput::Peer(self.get(region, store).cloned()));
            }
            HotTask::IsHot { meta, min_degree } => {
                return Ok(HotTaskOutput::IsHot(meta.peers.iter().any(|p| {
                    self.get(meta.id, p.store_id)
                        .is_some_and(|p| p.hot_degree >= min_degree)
                })));
            }
        };
        for action in actions {
            self.update_stat_at(action, now);
        }
        Ok(HotTaskOutput::Applied)
    }
}

fn task_region(meta: RegionMeta, leader_store: u64) -> RegionInfo {
    let leader = (leader_store != 0).then_some(Peer {
        store_id: leader_store,
        ..Default::default()
    });
    RegionInfo::new(meta, leader, vec![], vec![])
}

const PENDING: u8 = 0;
const RUNNING: u8 = 1;
const CANCELED: u8 = 2;
const FINISHED: u8 = 3;

/// Acknowledgement of execution, separate from successful queue admission.
/// Dropping a receipt does not cancel an admitted task. Use cancel explicitly.
pub struct HotTaskReceipt {
    state: Arc<AtomicU8>,
    result: oneshot::Receiver<Result<HotTaskOutput, HotTaskError>>,
}
impl HotTaskReceipt {
    /// Returns true only when execution was prevented before the worker claimed it.
    pub fn cancel(&self) -> bool {
        self.state
            .compare_exchange(PENDING, CANCELED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    /// Waits for an execution result. Channel loss is an error, never Applied.
    pub async fn wait(self) -> Result<HotTaskOutput, HotTaskError> {
        self.result
            .await
            .unwrap_or(Err(HotTaskError::WorkerStopped))
    }
}
struct PendingTask {
    task: HotTask,
    now: i64,
    state: Arc<AtomicU8>,
    result: oneshot::Sender<Result<HotTaskOutput, HotTaskError>>,
}

/// Read/write FIFO workers spawned on a host-owned Tokio runtime.
/// Call cancel then shutdown to wait for both workers and recover final state.
/// Drop requests cancellation; explicit shutdown supplies join evidence.
pub struct HotCache {
    read: mpsc::Sender<PendingTask>,
    write: mpsc::Sender<PendingTask>,
    cancel: watch::Sender<bool>,
    workers: Option<[JoinHandle<HotPeerCache>; 2]>,
}
impl HotCache {
    /// Starts independent directional queues. Capacity must be nonzero.
    pub fn new(handle: &Handle, capacity: usize) -> Self {
        assert!(capacity > 0, "hot queue capacity must be positive");
        let (read, read_rx) = mpsc::channel(capacity);
        let (write, write_rx) = mpsc::channel(capacity);
        let (cancel, canceled) = watch::channel(false);
        let workers = [
            handle.spawn(run_worker(
                HotPeerCache::new(RWType::READ),
                read_rx,
                canceled.clone(),
            )),
            handle.spawn(run_worker(
                HotPeerCache::new(RWType::WRITE),
                write_rx,
                canceled,
            )),
        ];
        Self {
            read,
            write,
            cancel,
            workers: Some(workers),
        }
    }
    /// Nonblocking admission. Only the returned receipt can confirm application.
    pub fn submit(
        &self,
        kind: RWType,
        task: HotTask,
        now: i64,
    ) -> Result<HotTaskReceipt, HotTaskError> {
        if *self.cancel.borrow() {
            return Err(HotTaskError::Closed);
        }
        let sender = match kind {
            RWType::READ => &self.read,
            RWType::WRITE => &self.write,
            _ => return Err(HotTaskError::WrongDirection),
        };
        let state = Arc::new(AtomicU8::new(PENDING));
        let (result, receiver) = oneshot::channel();
        sender
            .try_send(PendingTask {
                task,
                now,
                state: state.clone(),
                result,
            })
            .map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => HotTaskError::Full,
                mpsc::error::TrySendError::Closed(_) => HotTaskError::Closed,
            })?;
        Ok(HotTaskReceipt {
            state,
            result: receiver,
        })
    }
    /// Admits expiration on both queues, then write flow, in Go order.
    /// Each result is independent: partial admission is visible to the host.
    pub fn check_region_flow_async(
        &self,
        region: &RegionInfo,
        now: i64,
    ) -> [Result<HotTaskReceipt, HotTaskError>; 3] {
        let (expired, write) = new_region_flow_tasks(region);
        [
            self.submit(RWType::WRITE, expired.clone(), now),
            self.submit(RWType::READ, expired, now),
            self.submit(RWType::WRITE, write, now),
        ]
    }
    /// Reads a cloned peer after all previously admitted tasks in that direction.
    pub async fn get_hot_peer_stat(
        &self,
        kind: RWType,
        region: u64,
        store: u64,
    ) -> Result<Option<HotPeerStat>, HotTaskError> {
        match self
            .submit(kind, HotTask::Peer { region, store }, 0)?
            .wait()
            .await?
        {
            HotTaskOutput::Peer(peer) => Ok(peer),
            _ => unreachable!("peer task result"),
        }
    }
    /// Reads filtered snapshots after preceding directional reports.
    pub async fn get_hot_peer_stats(
        &self,
        kind: RWType,
        min_degree: i64,
    ) -> Result<BTreeMap<u64, Vec<HotPeerStat>>, HotTaskError> {
        match self
            .submit(kind, HotTask::Stats { min_degree }, 0)?
            .wait()
            .await?
        {
            HotTaskOutput::Stats(stats) => Ok(stats),
            _ => unreachable!("stats task result"),
        }
    }
    /// Checks membership in both directions, retaining admission/cancellation errors.
    pub async fn is_region_hot(
        &self,
        region: &RegionInfo,
        min_degree: i64,
    ) -> Result<bool, HotTaskError> {
        let task = HotTask::IsHot {
            meta: region.meta().clone(),
            min_degree,
        };
        let read = self.submit(RWType::READ, task.clone(), 0)?;
        let write = self.submit(RWType::WRITE, task, 0)?;
        let r = read.wait().await?;
        let w = write.wait().await?;
        Ok(matches!(r, HotTaskOutput::IsHot(true)) || matches!(w, HotTaskOutput::IsHot(true)))
    }
    /// Stops new admission and asks workers to reject queued tasks.
    /// A command already claimed may finish and reports its real result.
    pub fn cancel(&self) {
        self.cancel.send_replace(true);
    }
    /// Cancels, joins both workers even if one fails, and returns read/write caches.
    /// Repeated calls safely return None. The host runtime must remain running.
    pub async fn shutdown(&mut self) -> Result<Option<[HotPeerCache; 2]>, HotTaskError> {
        self.cancel();
        let Some([read, write]) = self.workers.take() else {
            return Ok(None);
        };
        let (read, write) = tokio::join!(read, write);
        Ok(Some([
            read.map_err(|_| HotTaskError::WorkerStopped)?,
            write.map_err(|_| HotTaskError::WorkerStopped)?,
        ]))
    }
}
impl Drop for HotCache {
    fn drop(&mut self) {
        self.cancel();
    }
}
async fn run_worker(
    mut cache: HotPeerCache,
    mut queue: mpsc::Receiver<PendingTask>,
    mut cancel: watch::Receiver<bool>,
) -> HotPeerCache {
    loop {
        let pending = tokio::select! {
            biased;
            _ = cancel.changed() => break,
            task = queue.recv() => match task { Some(task) => task, None => break },
        };
        let result = if *cancel.borrow()
            || pending
                .state
                .compare_exchange(PENDING, RUNNING, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            Err(HotTaskError::Canceled)
        } else {
            cache.run_task(pending.task, pending.now)
        };
        pending.state.store(FINISHED, Ordering::Release);
        let _ = pending.result.send(result);
    }
    queue.close();
    while let Some(pending) = queue.recv().await {
        pending.state.store(CANCELED, Ordering::Release);
        let _ = pending.result.send(Err(HotTaskError::Canceled));
    }
    cache
}
