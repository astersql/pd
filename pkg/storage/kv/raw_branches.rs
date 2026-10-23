// Copyright 2026 AsterSQL.
use crate::*;
use tokio_util::sync::CancellationToken;

/// A single-use RawTxn builder over the canonical metadata backend.
/// Branches contain point operations; comparisons run at the commit point.
pub struct RawTxn<'a> {
    store: &'a dyn MetaStore,
    conditions: Vec<Compare>,
    then_writes: Vec<RawTxnOp>,
    else_writes: Vec<RawTxnOp>,
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
        self.then_writes = writes.into_iter().map(Into::into).collect();
        self
    }
    pub fn or_else(mut self, writes: Vec<Mutation>) -> Self {
        self.else_writes = writes.into_iter().map(Into::into).collect();
        self
    }
    /// Set the ordered success branch, including point reads.
    pub fn then_ops(mut self, ops: Vec<RawTxnOp>) -> Self {
        self.then_writes = ops;
        self
    }
    /// Set the ordered failure branch, including point reads.
    pub fn else_ops(mut self, ops: Vec<RawTxnOp>) -> Self {
        self.else_writes = ops;
        self
    }
    pub async fn commit(self) -> Result<RawTxnResponse> {
        self.store
            .transact_raw(self.conditions, self.then_writes, self.else_writes)
            .await
    }
    /// Pre-cancellation never dispatches. Once dispatched, an unavailable/cancelled
    /// transport outcome may be unknown; callers must not blindly retry writes.
    pub async fn commit_with_cancel(self, cancel: &CancellationToken) -> Result<RawTxnResponse> {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(StoreError::Cancelled),
            result = self.commit() => result,
        }
    }
}
