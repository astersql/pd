// Copyright 2026 AsterSQL.
use crate::{ElectionError, Leadership};
use astersql_pd_storage_domain::{MetaStore, Read};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
/// Host persistence is restricted to the active immutable leadership term.
/// Administrative writes and mutating failure branches are denied.
pub struct LeadershipStore {
    inner: Arc<dyn MetaStore>,
    leader: Arc<Leadership>,
    cancel: CancellationToken,
}
impl LeadershipStore {
    pub fn new(
        inner: Arc<dyn MetaStore>,
        leader: Arc<Leadership>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            inner,
            leader,
            cancel,
        }
    }
}
#[async_trait::async_trait]
impl MetaStore for LeadershipStore {
    async fn read(
        &self,
        read: Read,
    ) -> astersql_pd_storage_domain::Result<astersql_pd_storage_domain::Snapshot> {
        tokio::select! { biased;
            _ = self.cancel.cancelled() => Err(astersql_pd_storage_domain::StoreError::Cancelled),
            result = self.inner.read(read) => result,
        }
    }
    async fn transact(
        &self,
        compares: Vec<astersql_pd_storage_domain::Compare>,
        writes: Vec<astersql_pd_storage_domain::Mutation>,
    ) -> astersql_pd_storage_domain::Result<astersql_pd_storage_domain::Commit> {
        if self.cancel.is_cancelled() || self.leader.is_expired() {
            return Err(astersql_pd_storage_domain::StoreError::Cancelled);
        }
        self.leader
            .transact(compares, writes)
            .await
            .map_err(|error| match error {
                ElectionError::Store(error) => error,
                ElectionError::Cancelled => astersql_pd_storage_domain::StoreError::Cancelled,
                _ => astersql_pd_storage_domain::StoreError::PermissionDenied,
            })
    }
    async fn transact_branches(
        &self,
        _: Vec<astersql_pd_storage_domain::Compare>,
        _: Vec<astersql_pd_storage_domain::Mutation>,
        _: Vec<astersql_pd_storage_domain::Mutation>,
    ) -> astersql_pd_storage_domain::Result<astersql_pd_storage_domain::Commit> {
        Err(astersql_pd_storage_domain::StoreError::PermissionDenied)
    }
    async fn transact_raw(
        &self,
        compares: Vec<astersql_pd_storage_domain::Compare>,
        ops: Vec<astersql_pd_storage_domain::RawTxnOp>,
        otherwise: Vec<astersql_pd_storage_domain::RawTxnOp>,
    ) -> astersql_pd_storage_domain::Result<astersql_pd_storage_domain::RawTxnResponse> {
        if !otherwise.is_empty() {
            return Err(astersql_pd_storage_domain::StoreError::PermissionDenied);
        }
        if self.cancel.is_cancelled() || self.leader.is_expired() {
            return Err(astersql_pd_storage_domain::StoreError::Cancelled);
        }
        tokio::select! {biased;
            _=self.cancel.cancelled()=>Err(astersql_pd_storage_domain::StoreError::Cancelled),
            result=self.leader.transact_raw(compares,ops)=>result.map_err(|error|match error {
                ElectionError::Store(error)=>error,
                ElectionError::Cancelled=>astersql_pd_storage_domain::StoreError::Cancelled,
                _=>astersql_pd_storage_domain::StoreError::PermissionDenied,
            }),
        }
    }
    async fn watch(
        &self,
        prefix: Vec<u8>,
        revision: astersql_pd_storage_domain::Revision,
    ) -> astersql_pd_storage_domain::Result<Box<dyn astersql_pd_storage_domain::MetaWatch>> {
        self.inner.watch(prefix, revision).await
    }
    async fn grant(
        &self,
        _: i64,
    ) -> astersql_pd_storage_domain::Result<astersql_pd_storage_domain::Lease> {
        Err(astersql_pd_storage_domain::StoreError::PermissionDenied)
    }
    async fn keep_alive(
        &self,
        _: astersql_pd_storage_domain::LeaseId,
    ) -> astersql_pd_storage_domain::Result<astersql_pd_storage_domain::Lease> {
        Err(astersql_pd_storage_domain::StoreError::PermissionDenied)
    }
    async fn revoke(
        &self,
        _: astersql_pd_storage_domain::LeaseId,
    ) -> astersql_pd_storage_domain::Result<()> {
        Err(astersql_pd_storage_domain::StoreError::PermissionDenied)
    }
    async fn ttl(
        &self,
        lease: astersql_pd_storage_domain::LeaseId,
    ) -> astersql_pd_storage_domain::Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(
        &self,
        _: astersql_pd_storage_domain::Revision,
    ) -> astersql_pd_storage_domain::Result<()> {
        Err(astersql_pd_storage_domain::StoreError::PermissionDenied)
    }
}
