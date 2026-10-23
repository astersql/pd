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
    window: Mutex<Window>,
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
        if !leader.uses_store(&store) {
            return Err(TsoError::StoreMismatch);
        }
        if save_interval_millis < 2 || save_interval_millis > MAX_PHYSICAL {
            return Err(TsoError::InvalidArgument);
        }
        let oracle = Self {
            key: format!("/pd/{}/timestamp", leader.cluster_id()).into_bytes(),
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
        Ok(oracle)
    }
    async fn initialize(&self) -> Result<(), TsoError> {
        let mut window = self.window.lock().await;
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
            let commit = self
                .leader
                .transact(
                    vec![compare],
                    vec![Mutation::put(&self.key, &end.to_be_bytes())],
                )
                .await?;
            if commit.succeeded {
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
                vec![Compare::Value(
                    self.key.clone(),
                    saved.to_be_bytes().to_vec(),
                )],
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
        let physical = if now > window.timestamp.physical + 1 {
            now
        } else if window.overflowed || window.timestamp.logical > MAX_LOGICAL / 2 {
            window.timestamp.physical + 1
        } else {
            self.fence(window.saved_nanos, vec![]).await?;
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
        self.fence(window.saved_nanos, writes).await?;
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
