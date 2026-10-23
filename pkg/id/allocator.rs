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
/// Each instance owns a disjoint reserved window. Unused IDs are abandoned on restart.
pub struct Allocator {
    store: Arc<dyn MetaStore>,
    leader: Arc<Leadership>,
    cancel: CancellationToken,
    key: Vec<u8>,
    step: u64,
    window: Mutex<Window>,
}
impl Allocator {
    pub fn new(
        store: Arc<dyn MetaStore>,
        leader: Arc<Leadership>,
        cancel: CancellationToken,
        step: u64,
    ) -> Result<Self, AllocatorError> {
        if !leader.uses_store(&store) {
            return Err(AllocatorError::StoreMismatch);
        }
        let key = format!("/pd/{}/alloc_id", leader.cluster_id()).into_bytes();
        Ok(Self {
            store,
            leader,
            cancel,
            key,
            step: if step == 0 { DEFAULT_ALLOC_STEP } else { step },
            window: Mutex::new(Window::default()),
        })
    }
    /// Return one globally unique ID after validating the term at a linearization point.
    pub async fn alloc(&self) -> Result<u64, AllocatorError> {
        tokio::select! {biased; _=self.cancel.cancelled()=>Err(AllocatorError::Cancelled), result=self.allocate()=>result}
    }
    async fn allocate(&self) -> Result<u64, AllocatorError> {
        let mut window = self.window.lock().await;
        self.leader.validate().await?;
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
                if base == u64::MAX {
                    return Err(AllocatorError::Exhausted);
                }
                let end = base.saturating_add(self.step);
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
        Ok(window.next)
    }
}
