// Copyright 2026 AsterSQL.
use astersql_pd_member::{Leadership, MemberError};
use astersql_pd_storage_kv::{Compare, Consistency, MetaStore, Mutation, Read, StoreError};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Go classic TSO uses 18 logical bits, with this exclusive limit.
pub const MAX_LOGICAL: u32 = 1 << 18;
const NANOS_PER_MILLI: u64 = 1_000_000;
// Go stores UnixNano in an int64. Reject out-of-range values rather than wrap.
const MAX_PHYSICAL: u64 = i64::MAX as u64 / NANOS_PER_MILLI;

/// Physical milliseconds and logical counter; ordering is the PD TSO ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    pub physical: u64,
    pub logical: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum TsoError {
    #[error("[PD:tso:ErrResetUserTimestamp]reset user timestamp failed, {0}")]
    ResetRejected(&'static str),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Member(#[from] MemberError),
    #[error("operation cancelled")]
    Cancelled,
    #[error("invalid TSO request or window interval")]
    InvalidArgument,
    #[error("invalid persisted timestamp window")]
    CorruptBoundary,
    #[error("timestamp range exhausted")]
    Exhausted,
    #[error("system clock precedes Unix epoch")]
    Clock,
    #[error("timestamp window replaced; recreate the oracle")]
    WindowLost,
    #[error("timestamp window exhausted; caller must update before retrying")]
    WindowExhausted,
    #[error("leadership and oracle must share a metadata store")]
    StoreMismatch,
}

/// Injectable wall clock. Runtime scheduling remains the caller's responsibility.
pub trait Clock: Send + Sync {
    fn now_millis(&self) -> Result<u64, TsoError>;
}
/// Production Unix wall clock, without a private runtime or background worker.
pub struct SystemClock;
impl Clock for SystemClock {
    fn now_millis(&self) -> Result<u64, TsoError> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| TsoError::Clock)?
            .as_millis();
        u64::try_from(millis).map_err(|_| TsoError::Exhausted)
    }
}
#[derive(Clone, Copy)]
struct Window {
    timestamp: Timestamp,
    saved_nanos: u64,
    overflowed: bool,
}

/// A classic single-group oracle. Each allocation fences its term and durable window.
/// A second instance in the same term invalidates the first instead of issuing overlaps.
/// Schedule `update()` on the host's shared runtime; it is also needed after WindowExhausted.
pub struct Oracle {
    store: Arc<dyn MetaStore>,
    leader: Arc<Leadership>,
    clock: Arc<dyn Clock>,
    cancel: CancellationToken,
    key: Vec<u8>,
    interval: u64,
    metadata_guard: Vec<Compare>,
    window: Mutex<Window>,
    metrics: Option<crate::OracleMetrics>,
}
impl Oracle {
    /// Reserve a durable window before publishing any timestamp. Unused time is abandoned.
    pub async fn new(
        store: Arc<dyn MetaStore>,
        leader: Arc<Leadership>,
        clock: Arc<dyn Clock>,
        cancel: CancellationToken,
        save_interval_millis: u64,
    ) -> Result<Self, TsoError> {
        Self::new_with_metrics(store, leader, clock, cancel, save_interval_millis, None).await
    }
    /// Reserve the initial window with caller-owned metric handles.
    pub async fn new_with_metrics(
        store: Arc<dyn MetaStore>,
        leader: Arc<Leadership>,
        clock: Arc<dyn Clock>,
        cancel: CancellationToken,
        save_interval_millis: u64,
        metrics: Option<crate::OracleMetrics>,
    ) -> Result<Self, TsoError> {
        Self::new_for_group_with_metrics(
            store,
            leader,
            clock,
            cancel,
            save_interval_millis,
            0,
            metrics,
            vec![],
        )
        .await
    }
    /// Reserve a canonical group's durable window under its own election term.
    pub async fn new_for_group(
        store: Arc<dyn MetaStore>,
        leader: Arc<Leadership>,
        clock: Arc<dyn Clock>,
        cancel: CancellationToken,
        save_interval_millis: u64,
        group_id: u32,
    ) -> Result<Self, TsoError> {
        Self::new_for_group_with_metrics(
            store,
            leader,
            clock,
            cancel,
            save_interval_millis,
            group_id,
            None,
            vec![],
        )
        .await
    }
    /// Reserve a window whose reads and writes are fenced by authoritative group metadata.
    pub async fn new_for_group_guarded(
        store: Arc<dyn MetaStore>,
        leader: Arc<Leadership>,
        clock: Arc<dyn Clock>,
        cancel: CancellationToken,
        interval: u64,
        group: u32,
        guard: Vec<Compare>,
    ) -> Result<Self, TsoError> {
        Self::new_for_group_with_metrics(store, leader, clock, cancel, interval, group, None, guard)
            .await
    }
    async fn new_for_group_with_metrics(
        store: Arc<dyn MetaStore>,
        leader: Arc<Leadership>,
        clock: Arc<dyn Clock>,
        cancel: CancellationToken,
        save_interval_millis: u64,
        group_id: u32,
        metrics: Option<crate::OracleMetrics>,
        metadata_guard: Vec<Compare>,
    ) -> Result<Self, TsoError> {
        if !leader.uses_store(&store) {
            return Err(TsoError::StoreMismatch);
        }
        if save_interval_millis < 2 || save_interval_millis > MAX_PHYSICAL {
            return Err(TsoError::InvalidArgument);
        }
        let oracle = Self {
            metrics,
            metadata_guard,
            key: astersql_pd_keypath::ClusterPaths::new(leader.cluster_id())
                .timestamp_path(group_id)
                .into_bytes(),
            store,
            leader,
            clock,
            cancel,
            interval: save_interval_millis,
            window: Mutex::new(Window {
                timestamp: Timestamp {
                    physical: 0,
                    logical: 0,
                },
                saved_nanos: 0,
                overflowed: false,
            }),
        };
        tokio::select! {biased;
            _ = oracle.cancel.cancelled() => return Err(TsoError::Cancelled),
            result = oracle.initialize() => result?,
        }
        if let Some(metrics) = &oracle.metrics {
            metrics.role.set(1.);
        }
        Ok(oracle)
    }
    async fn initialize(&self) -> Result<(), TsoError> {
        let mut window = self.window.lock().await;
        if let Some(metrics) = &self.metrics {
            metrics.event("sync");
        }
        loop {
            self.leader.validate().await?;
            let snapshot = self
                .store
                .read(Read::prefix(&self.key, Consistency::Linearizable))
                .await?;
            let entry = snapshot.entries.iter().find(|e| e.key == self.key);
            let saved = match entry {
                Some(e) => {
                    let nanos = u64::from_be_bytes(
                        e.value
                            .as_slice()
                            .try_into()
                            .map_err(|_| TsoError::CorruptBoundary)?,
                    );
                    if nanos > i64::MAX as u64 {
                        return Err(TsoError::CorruptBoundary);
                    }
                    nanos
                }
                None => 0,
            };
            // Round up: Go can persist sub-millisecond times. Preserve the full 1ms guard.
            let physical = self
                .clock
                .now_millis()?
                .max(saved.div_ceil(NANOS_PER_MILLI) + 1);
            let end = self.end(physical)?;
            let compare = match entry {
                Some(e) => Compare::ModRevision(self.key.clone(), e.mod_revision),
                None => Compare::CreateRevision(self.key.clone(), 0),
            };
            let begin = std::time::Instant::now();
            let commit = self
                .leader
                .transact(
                    self.metadata_guard
                        .iter()
                        .cloned()
                        .chain([compare])
                        .collect(),
                    vec![Mutation::put(&self.key, &end.to_be_bytes())],
                )
                .await
                .map_err(|error| {
                    if let Some(metrics) = &self.metrics {
                        metrics.event("err_save_sync_ts");
                    }
                    error
                })?;
            if commit.succeeded {
                if let Some(metrics) = &self.metrics {
                    metrics.sync.observe(begin.elapsed().as_secs_f64());
                    metrics.event("save");
                }
                if let Some(metrics) = &self.metrics {
                    metrics.event("sync_ok");
                }
                *window = Window {
                    timestamp: Timestamp {
                        physical,
                        logical: 0,
                    },
                    saved_nanos: end,
                    overflowed: false,
                };
                return Ok(());
            }
            if !self.metadata_guard.is_empty() {
                return Err(TsoError::WindowLost);
            }
            tokio::task::yield_now().await;
        }
    }
    fn end(&self, physical: u64) -> Result<u64, TsoError> {
        let end = physical
            .checked_add(self.interval)
            .filter(|v| *v <= MAX_PHYSICAL)
            .ok_or(TsoError::Exhausted)?;
        Ok(end * NANOS_PER_MILLI)
    }
    async fn fence(&self, saved: u64, writes: Vec<Mutation>) -> Result<(), TsoError> {
        let commit = self
            .leader
            .transact(
                self.metadata_guard
                    .iter()
                    .cloned()
                    .chain([Compare::Value(
                        self.key.clone(),
                        saved.to_be_bytes().to_vec(),
                    )])
                    .collect(),
                writes,
            )
            .await?;
        if commit.succeeded {
            Ok(())
        } else {
            Err(TsoError::WindowLost)
        }
    }
    /// Allocate a batch and return its inclusive upper timestamp (logical-count+1..logical).
    /// Overflow advances 1ms only inside the durable window, as in Go getTS.
    pub async fn allocate(&self, count: u32) -> Result<Timestamp, TsoError> {
        if count == 0 || count >= MAX_LOGICAL {
            return Err(TsoError::InvalidArgument);
        }
        tokio::select! {biased;
            _ = self.cancel.cancelled() => Err(TsoError::Cancelled),
            result = self.allocate_inner(count) => result,
        }
    }
    async fn allocate_inner(&self, count: u32) -> Result<Timestamp, TsoError> {
        let mut window = self.window.lock().await;
        let mut next = window.timestamp;
        if next.logical + count >= MAX_LOGICAL {
            if let Some(metrics) = &self.metrics {
                metrics.event("logical_overflow");
            }
            next.physical = next
                .physical
                .checked_add(1)
                .ok_or(TsoError::Exhausted)?
                .max(self.clock.now_millis()?);
            if next.physical > MAX_PHYSICAL {
                return Err(TsoError::Exhausted);
            }
            next.logical = 0;
            // Overflow must not extend the durable window on the request path.
            if (next.physical + 1) * NANOS_PER_MILLI >= window.saved_nanos {
                self.fence(window.saved_nanos, vec![]).await?;
                window.overflowed = true;
                return Err(TsoError::WindowExhausted);
            }
        }
        self.fence(window.saved_nanos, vec![]).await?;
        next.logical += count;
        window.timestamp = next;
        Ok(next)
    }
    /// Reset the canonical window atomically, preserving Go's strict upper bound.
    /// Forced restore ignores older values and bypasses only the configured gap.
    pub async fn reset(&self, tso: u64, force: bool, max_gap_millis: i64) -> Result<(), TsoError> {
        tokio::select! {biased;
            _ = self.cancel.cancelled() => Err(TsoError::Cancelled),
            result = self.reset_inner(tso, force, max_gap_millis) => result,
        }
    }
    async fn reset_inner(
        &self,
        tso: u64,
        force: bool,
        max_gap_millis: i64,
    ) -> Result<(), TsoError> {
        let mut window = self.window.lock().await;
        self.leader.validate().await.map_err(|error| {
            if let Some(metrics) = &self.metrics {
                metrics.event("err_lease_reset_ts");
            }
            error
        })?;
        let next = Timestamp {
            physical: tso >> 18,
            logical: (tso & (u64::from(MAX_LOGICAL) - 1)) as u32,
        };
        if next <= window.timestamp {
            self.fence(window.saved_nanos, vec![]).await?;
            if let Some(metrics) = &self.metrics {
                metrics.event(if next.physical < window.timestamp.physical {
                    "err_reset_physical_small_ts"
                } else {
                    "err_reset_logical_small_ts"
                });
            }
            if force {
                return Ok(());
            }
            return Err(TsoError::ResetRejected(
                if next.physical < window.timestamp.physical {
                    "the specified ts is smaller than now"
                } else {
                    "the specified counter is smaller than now"
                },
            ));
        }
        if !force
            && i128::from(next.physical - window.timestamp.physical) >= i128::from(max_gap_millis)
        {
            if let Some(metrics) = &self.metrics {
                metrics.event("err_reset_large_ts");
            }
            return Err(TsoError::ResetRejected(
                "the specified ts is too larger than now",
            ));
        }
        let end = self.end(next.physical)?;
        let extend = (next.physical + 1) * NANOS_PER_MILLI >= window.saved_nanos;
        let writes = if extend {
            vec![Mutation::put(&self.key, &end.to_be_bytes())]
        } else {
            vec![]
        };
        let begin = std::time::Instant::now();
        self.fence(window.saved_nanos, writes)
            .await
            .map_err(|error| {
                if extend {
                    if let Some(metrics) = &self.metrics {
                        metrics.event("err_save_reset_ts");
                    }
                }
                error
            })?;
        if let Some(metrics) = &self.metrics {
            if extend {
                metrics.reset.observe(begin.elapsed().as_secs_f64());
            }
            metrics.event("reset_tso_ok");
        }
        window.timestamp = next;
        window.overflowed = false;
        if extend {
            window.saved_nanos = end;
        }
        Ok(())
    }
    /// Advance physical time and extend the saved window before using it. Safe on rollback.
    /// Host scheduling replaces Go's update loop; no worker is detached by this library.
    pub async fn update(&self) -> Result<(), TsoError> {
        tokio::select! {biased;
            _ = self.cancel.cancelled() => Err(TsoError::Cancelled),
            result = self.update_inner() => result,
        }
    }
    async fn update_inner(&self) -> Result<(), TsoError> {
        let mut window = self.window.lock().await;
        let now = self.clock.now_millis()?;
        if let Some(metrics) = &self.metrics {
            metrics.physical.set(window.timestamp.physical as f64);
            if now < window.timestamp.physical {
                metrics.event("system_time_slow");
            }
            if now.saturating_sub(window.timestamp.physical) > 150 {
                metrics.event("slow_save");
            }
            metrics
                .gap
                .set((i128::from(now) - i128::from(window.timestamp.physical)) as f64);
        }
        let physical = if now > window.timestamp.physical + 1 {
            now
        } else if window.overflowed || window.timestamp.logical > MAX_LOGICAL / 2 {
            window.timestamp.physical + 1
        } else {
            self.fence(window.saved_nanos, vec![]).await?;
            if let Some(metrics) = &self.metrics {
                metrics.event("skip_save");
            }
            return Ok(());
        };
        // Checked before multiplication, including arbitrary injected clock values.
        let end = self.end(physical)?;
        let extend = (physical + 1) * NANOS_PER_MILLI >= window.saved_nanos;
        let writes = if extend {
            vec![Mutation::put(&self.key, &end.to_be_bytes())]
        } else {
            vec![]
        };
        let begin = std::time::Instant::now();
        self.fence(window.saved_nanos, writes)
            .await
            .map_err(|error| {
                if extend {
                    if let Some(metrics) = &self.metrics {
                        metrics.event("err_save_update_ts");
                    }
                }
                error
            })?;
        if extend {
            if let Some(metrics) = &self.metrics {
                metrics.update.observe(begin.elapsed().as_secs_f64());
            }
        }
        if let Some(metrics) = &self.metrics {
            metrics.event("save");
        }
        window.timestamp = Timestamp {
            physical,
            logical: 0,
        };
        window.overflowed = false;
        if extend {
            window.saved_nanos = end;
        }
        Ok(())
    }
}
