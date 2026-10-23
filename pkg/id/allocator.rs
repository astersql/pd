// Copyright 2026 AsterSQL.
use astersql_pd_member::{Leadership, MemberError};
use astersql_pd_storage_kv::{Compare, Consistency, MetaStore, Mutation, Read, StoreError};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_ALLOC_STEP: u64 = 1000;
#[derive(Debug, thiserror::Error)]
pub enum AllocatorError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Member(#[from] MemberError),
    #[error("operation cancelled")]
    Cancelled,
    #[error("ID space exhausted")]
    Exhausted,
    #[error("invalid persisted ID boundary")]
    CorruptBoundary,
    #[error("leadership and allocator must share a metadata store")]
    StoreMismatch,
}
#[derive(Default)]
struct Window {
    next: u64,
    end: u64,
}
/// Ordinary reservations own disjoint windows; unused IDs are abandoned on restart.
/// Explicit `set_base` can overwrite reservations and reuse IDs, matching Go SetBase.
pub struct Allocator {
    store: Arc<dyn MetaStore>,
    leader: Arc<Leadership>,
    cancel: CancellationToken,
    key: Vec<u8>,
    step: u64,
    effective_end: u64,
    window: Mutex<Window>,
}
impl Allocator {
    /// Construct an allocator with an explicit scope and keyspace kernel mode.
    pub fn with_namespace(
        store: Arc<dyn MetaStore>,
        leader: Arc<Leadership>,
        cancel: CancellationToken,
        step: u64,
        namespace: crate::AllocatorNamespace,
    ) -> Result<Self, AllocatorError> {
        if !leader.uses_store(&store) {
            return Err(AllocatorError::StoreMismatch);
        }
        let key = namespace.key(leader.cluster_id());
        Ok(Self {
            store,
            leader,
            cancel,
            key,
            step: if step == 0 { DEFAULT_ALLOC_STEP } else { step },
            effective_end: namespace.effective_end(),
            window: Mutex::new(Window::default()),
        })
    }
    /// Construct the existing global allocator.
    pub fn new(
        store: Arc<dyn MetaStore>,
        leader: Arc<Leadership>,
        cancel: CancellationToken,
        step: u64,
    ) -> Result<Self, AllocatorError> {
        Self::with_namespace(
            store,
            leader,
            cancel,
            step,
            crate::AllocatorNamespace::Global,
        )
    }
    /// Return the next ID after validating the term at a linearization point.
    pub async fn alloc(&self) -> Result<u64, AllocatorError> {
        self.alloc_count(1).await.map(|(last, _)| last)
    }
    /// Reset the base and persist a new window, matching Go SetBase.
    ///
    /// Lower and equal bases are allowed. The previous durable boundary is neither
    /// read nor compared, so callers must coordinate resets to avoid reusing IDs.
    /// A failed write retains the old base and the requested end, as in Go;
    /// transport errors are returned without retrying an uncertain commit.
    pub async fn set_base(&self, new_base: u64) -> Result<(), AllocatorError> {
        let reset = async {
            let mut window = self.window.lock().await;
            if new_base >= self.effective_end {
                return Err(AllocatorError::Exhausted);
            }
            window.end = new_base;
            // Go uint64 addition wraps here, unlike ordinary Rust reservations.
            let end = new_base.wrapping_add(self.step).min(self.effective_end);
            let commit = self
                .leader
                .transact(vec![], vec![Mutation::put(&self.key, &end.to_be_bytes())])
                .await?;
            if !commit.succeeded {
                return Err(AllocatorError::Member(MemberError::NotLeader));
            }
            window.next = end.wrapping_sub(self.step);
            window.end = end;
            Ok(())
        };
        tokio::select! {biased; _=self.cancel.cancelled()=>Err(AllocatorError::Cancelled), result=reset=>result}
    }
    /// Return the last ID and requested count, holding the window lock for the batch.
    ///
    /// Zero count reads the current local base without storage or term validation.
    /// Errors preserve any IDs already consumed and reservations already committed.
    pub async fn alloc_count(&self, count: u32) -> Result<(u64, u32), AllocatorError> {
        if count == 0 {
            return Ok((self.window.lock().await.next, 0));
        }
        tokio::select! {biased; _=self.cancel.cancelled()=>Err(AllocatorError::Cancelled), result=self.allocate(count)=>result}
    }
    async fn allocate(&self, count: u32) -> Result<(u64, u32), AllocatorError> {
        let mut window = self.window.lock().await;
        for _ in 0..count {
            self.leader.validate().await?;
            if window.next >= self.effective_end {
                return Err(AllocatorError::Exhausted);
            }
            if window.next == window.end {
                loop {
                    let snapshot = self
                        .store
                        .read(Read::prefix(&self.key, Consistency::Linearizable))
                        .await?;
                    let entry = snapshot.entries.iter().find(|e| e.key == self.key);
                    let base = match entry {
                        Some(e) => u64::from_be_bytes(
                            e.value
                                .as_slice()
                                .try_into()
                                .map_err(|_| AllocatorError::CorruptBoundary)?,
                        ),
                        None => 0,
                    };
                    if base >= self.effective_end {
                        return Err(AllocatorError::Exhausted);
                    }
                    let end = base.saturating_add(self.step).min(self.effective_end);
                    let compare = match entry {
                        Some(e) => Compare::ModRevision(self.key.clone(), e.mod_revision),
                        None => Compare::CreateRevision(self.key.clone(), 0),
                    };
                    // Never retry transport errors: a failed response may hide a committed reservation.
                    let commit = self
                        .leader
                        .transact(
                            vec![compare],
                            vec![Mutation::put(&self.key, &end.to_be_bytes())],
                        )
                        .await?;
                    if commit.succeeded {
                        window.next = base;
                        window.end = end;
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            }
            window.next += 1;
        }
        Ok((window.next, count))
    }
}
