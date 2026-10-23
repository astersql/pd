// Copyright 2026 AsterSQL.
use crate::*;
use tokio_util::sync::CancellationToken;

/// A single-use RawTxn write builder over the canonical metadata backend.
/// Branches contain point puts/deletes; comparisons run at the commit point.
pub struct RawTxn<'a> {
    store: &'a dyn MetaStore,
    conditions: Vec<Compare>,
    then_writes: Vec<Mutation>,
    else_writes: Vec<Mutation>,
}
impl<'a> RawTxn<'a> {
    pub fn new(store: &'a dyn MetaStore) -> Self {
        Self {
            store,
            conditions: vec![],
            then_writes: vec![],
            else_writes: vec![],
        }
    }
    pub fn when(mut self, conditions: Vec<RawTxnCondition>) -> Self {
        self.conditions = conditions.into_iter().map(Into::into).collect();
        self
    }
    pub fn and_then(mut self, writes: Vec<Mutation>) -> Self {
        self.then_writes = writes;
        self
    }
    pub fn or_else(mut self, writes: Vec<Mutation>) -> Self {
        self.else_writes = writes;
        self
    }
    pub async fn commit(self) -> Result<Commit> {
        self.store
            .transact_branches(self.conditions, self.then_writes, self.else_writes)
            .await
    }
    /// Pre-cancellation never dispatches. Once dispatched, an unavailable/cancelled
    /// transport outcome may be unknown; callers must not blindly retry writes.
    pub async fn commit_with_cancel(self, cancel: &CancellationToken) -> Result<Commit> {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(StoreError::Cancelled),
            result = self.commit() => result,
        }
    }
}
