// Copyright 2026 AsterSQL.
//! Observe real backend operations without changing their transactions or errors.
use astersql_pd_storage::*;
use prometheus::{Counter, Histogram};
use std::{sync::Arc, time::Instant};
pub(crate) struct MeteredStore {
    inner: Arc<dyn MetaStore>,
    counters: [Counter; 2],
    durations: [Histogram; 2],
}
impl MeteredStore {
    pub(crate) fn new(
        inner: Arc<dyn MetaStore>,
        scope: &astersql_pd_metricutil::ClassicMetrics,
    ) -> Self {
        Self {
            inner,
            counters: ["success", "failed"]
                .map(|result| scope.txn_counter.with_label_values(&[result])),
            durations: ["success", "failed"]
                .map(|result| scope.txn_duration.with_label_values(&[result])),
        }
    }
    fn begin(&self) -> Observation<'_> {
        Observation {
            store: self,
            begin: Instant::now(),
            result: 1,
        }
    }
}
struct Observation<'a> {
    store: &'a MeteredStore,
    begin: Instant,
    result: usize,
}
impl Observation<'_> {
    fn complete<T>(&mut self, result: &Result<T>) {
        self.result = usize::from(result.is_err());
    }
}
impl Drop for Observation<'_> {
    fn drop(&mut self) {
        self.store.counters[self.result].inc();
        self.store.durations[self.result].observe(self.begin.elapsed().as_secs_f64());
    }
}
#[async_trait::async_trait]
impl MetaStore for MeteredStore {
    async fn consensus_cluster_id(&self) -> Result<Option<u64>> {
        self.inner.consensus_cluster_id().await
    }
    async fn consensus_leader_id(&self) -> Result<Option<u64>> {
        self.inner.consensus_leader_id().await
    }
    async fn backend_health(&self) -> Vec<BackendHealth> {
        self.inner.backend_health().await
    }
    async fn read(&self, request: Read) -> Result<Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(&self, compares: Vec<Compare>, writes: Vec<Mutation>) -> Result<Commit> {
        let mut observation = self.begin();
        let result = self.inner.transact(compares, writes).await;
        observation.complete(&result);
        result
    }
    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        yes: Vec<Mutation>,
        no: Vec<Mutation>,
    ) -> Result<Commit> {
        let mut observation = self.begin();
        let result = self.inner.transact_branches(compares, yes, no).await;
        observation.complete(&result);
        result
    }
    async fn transact_raw(
        &self,
        compares: Vec<Compare>,
        yes: Vec<RawTxnOp>,
        no: Vec<RawTxnOp>,
    ) -> Result<RawTxnResponse> {
        let mut observation = self.begin();
        let result = self.inner.transact_raw(compares, yes, no).await;
        observation.complete(&result);
        result
    }
    async fn watch(&self, prefix: Vec<u8>, revision: Revision) -> Result<Box<dyn MetaWatch>> {
        self.inner.watch(prefix, revision).await
    }
    async fn watch_with_previous(
        &self,
        prefix: Vec<u8>,
        revision: Revision,
    ) -> Result<Box<dyn MetaWatch>> {
        self.inner.watch_with_previous(prefix, revision).await
    }
    async fn grant(&self, ttl: i64) -> Result<Lease> {
        self.inner.grant(ttl).await
    }
    async fn keep_alive(&self, lease: LeaseId) -> Result<Lease> {
        self.inner.keep_alive(lease).await
    }
    async fn revoke(&self, lease: LeaseId) -> Result<()> {
        self.inner.revoke(lease).await
    }
    async fn ttl(&self, lease: LeaseId) -> Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: Revision) -> Result<()> {
        self.inner.compact(revision).await
    }
}
