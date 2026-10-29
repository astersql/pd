// Copyright 2026 AsterSQL.
//! Listener-independent leader terms. Retiring a term drains its workers before re-election.
use crate::{
    NodeConfig, NodeError,
    node::{Application, prepare_term},
};
use astersql_pd_member::{LeaderWatch, Member, MemberError};
use astersql_pd_server::{
    ClusterReadService, PdServices, region_heartbeat_adapter::*, store_heartbeat_adapter::*,
};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::sync::{Arc, RwLock, Weak};
use tokio_util::sync::CancellationToken;

pub(crate) struct RoleServices {
    identity: NodeConfig,
    store: Weak<dyn MetaStore>,
    term: RwLock<Option<Arc<Application>>>,
}
impl RoleServices {
    pub(crate) fn new(identity: NodeConfig, store: Arc<dyn MetaStore>) -> Self {
        Self {
            identity,
            store: Arc::downgrade(&store),
            term: RwLock::new(None),
        }
    }
    pub(crate) fn current(&self) -> Option<Arc<Application>> {
        self.term.read().unwrap().clone()
    }
    pub(crate) fn replace(&self, term: Option<Arc<Application>>) {
        *self.term.write().unwrap() = term;
    }
    async fn active(&self) -> Result<Arc<Application>, String> {
        let term = self.current().ok_or("not leader")?;
        term.healthy().await?;
        Ok(term)
    }
}
#[async_trait::async_trait]
impl PdServices for RoleServices {
    fn identity(&self) -> &NodeConfig {
        &self.identity
    }
    fn store_heartbeat_service(&self) -> Option<&dyn StoreHeartbeatService> {
        Some(self)
    }
    fn region_heartbeat_service(&self) -> Option<&dyn RegionHeartbeatService> {
        Some(self)
    }
    fn bucket_heartbeat_service(
        &self,
    ) -> Option<&dyn astersql_pd_server::bucket_heartbeat_adapter::BucketHeartbeatService> {
        Some(self)
    }
    fn cluster_read_service(&self) -> Option<&dyn ClusterReadService> {
        Some(self)
    }
    fn region_sync_handle(&self) -> Option<Arc<astersql_pd_server::sync_transport::RegionSyncer>> {
        self.current()?.region_sync_handle()
    }
    async fn is_bootstrapped(&self) -> Result<bool, String> {
        // Go GetRaftCluster returns nil on followers, including heartbeat streams
        // whose bootstrap check deliberately precedes request-role validation.
        if self.current().is_none() {
            return Ok(false);
        }
        astersql_pd_storage_endpoint::StorageEndpoint::new(
            self.store.upgrade().ok_or("server not started")?,
        )
        .load_meta(&astersql_pd_keypath::ClusterPaths::new(
            self.identity.cluster_id,
        ))
        .await
        .map(|meta| meta.is_some())
        .map_err(|error| error.to_string())
    }
    fn discovery_store(&self) -> Option<Arc<dyn MetaStore>> {
        self.store.upgrade()
    }
    async fn bootstrap(
        &self,
        store: astersql_pd_core::StoreMeta,
        region: astersql_pd_core::RegionMeta,
    ) -> Result<bool, String> {
        let term = self.active().await?;
        let backend = Arc::new(TermStore::new(
            self.store.upgrade().ok_or("server not started")?,
            term.leader.clone(),
            term.cancel.clone(),
        ));
        term.cluster.bootstrap(backend, store, region).await
    }
    async fn healthy(&self) -> Result<(), String> {
        self.active().await.map(|_| ())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        self.active().await?.alloc_id().await
    }
    async fn alloc_ids(&self, count: u32) -> Result<(u64, u32), String> {
        self.active().await?.alloc_ids(count).await
    }
    async fn timestamp(&self, count: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        self.active().await?.timestamp(count).await
    }
}
#[async_trait::async_trait]
impl StoreHeartbeatService for RoleServices {
    async fn store_heartbeat(
        &self,
        stats: astersql_pd_core::StoreStats,
    ) -> Result<astersql_pd_core::NodeState, StoreHeartbeatError> {
        let term = self.active().await.map_err(StoreHeartbeatError::Other)?;
        term.cluster.store_heartbeat(stats).await
    }
}
#[async_trait::async_trait]
impl RegionHeartbeatService for RoleServices {
    async fn bind_heartbeat(
        &self,
        store_id: u64,
        stream: Arc<dyn astersql_pd_schedule_hbstream::routing::HeartbeatStream>,
    ) -> Result<(), RegionHeartbeatError> {
        self.active()
            .await
            .map_err(RegionHeartbeatError)?
            .cluster
            .bind_heartbeat(store_id, stream)
            .await
    }

    fn region_store_state(&self, id: u64) -> Option<astersql_pd_core::NodeState> {
        self.current()?.cluster.region_store_state(id)
    }
    async fn region_heartbeat(
        &self,
        region: astersql_pd_core::RegionInfo,
    ) -> Result<(), RegionHeartbeatError> {
        let term = self.active().await.map_err(RegionHeartbeatError)?;
        term.cluster.region_heartbeat(region).await
    }
}
#[async_trait::async_trait]
impl ClusterReadService for RoleServices {
    async fn store(&self, id: u64) -> Result<Option<Arc<astersql_pd_core::StoreInfo>>, String> {
        self.active().await?.cluster.store(id).await
    }
    async fn region(&self, id: u64) -> Result<Option<Arc<astersql_pd_core::RegionInfo>>, String> {
        self.active().await?.cluster.region(id).await
    }
}
async fn retire(term: Arc<Application>, registry: &prometheus::Registry) -> Result<(), NodeError> {
    term.cancel.cancel();
    let stopped = term.cluster.shutdown().await;
    let resigned = term.leader.resign().await;
    let unregistered = registry.unregister(Box::new(term.requests.clone()));
    stopped?;
    resigned?;
    unregistered?;
    Ok(())
}
pub(crate) async fn run(
    services: Arc<RoleServices>,
    member: Member,
    store: Arc<dyn MetaStore>,
    config: NodeConfig,
    registry: Arc<prometheus::Registry>,
    cancel: CancellationToken,
) -> Result<(), NodeError> {
    let result: Result<(), NodeError> = async {
        loop {
            if cancel.is_cancelled() {
                break;
            }
            if let Some(term) = services.current() {
                let mut tick = tokio::time::interval(std::time::Duration::from_millis(50));
                let mut renew = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    let result = tokio::select! { biased;
                        _ = cancel.cancelled() => break,
                        _ = renew.tick() => term.leader.renew().await.map_err(|e| e.to_string()),
                        _ = tick.tick() => term.oracle.update().await.map_err(|e| e.to_string()),
                    };
                    if result.is_err() {
                        break;
                    }
                }
                services.replace(None);
                retire(term, &registry).await?;
                continue;
            }
            // Snapshot before watch creation prevents a missed delete between failed campaign
            // and subscription. A compacted/disconnected stream always restarts from a read.
            let key = format!("/pd/{}/leader", config.cluster_id).into_bytes();
            let snapshot = tokio::select! { biased;
                _ = cancel.cancelled() => break,
                result = store.read(Read::range(&key, b"", Consistency::Linearizable)) => result?,
            };
            if snapshot.entries.iter().any(|entry| entry.key == key) {
                let mut watch =
                    LeaderWatch::new(store.clone(), key.clone(), snapshot.revision + 1)?;
                let _ = watch.next(&cancel).await;
                continue;
            }
            match member.campaign(5).await {
                Ok(leader) => {
                    let term = prepare_term(
                        store.clone(),
                        config.clone(),
                        registry.clone(),
                        cancel.child_token(),
                        astersql_pd_core::BasicCluster::new(),
                        Arc::new(leader),
                    )
                    .await?;
                    services.replace(Some(term));
                }
                Err(MemberError::Contended) => {}
                Err(MemberError::Cancelled) if cancel.is_cancelled() => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    .await;
    if let Some(term) = services.current() {
        services.replace(None);
        let cleanup = retire(term, &registry).await;
        result?;
        cleanup?;
    } else {
        result?;
    }
    Ok(())
}

/// Scheduling persistence is restricted to fenced transactions. Administrative and
/// branch/raw transaction APIs are deliberately unavailable to this term scope.
/// The canonical heartbeat pipeline only needs read and transact.
pub(crate) struct TermStore {
    inner: Arc<dyn MetaStore>,
    leader: Arc<astersql_pd_member::Leadership>,
    cancel: CancellationToken,
}
impl TermStore {
    pub(crate) fn new(
        inner: Arc<dyn MetaStore>,
        leader: Arc<astersql_pd_member::Leadership>,
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
impl MetaStore for TermStore {
    async fn read(&self, read: Read) -> astersql_pd_storage::Result<astersql_pd_storage::Snapshot> {
        tokio::select! { biased;
            _ = self.cancel.cancelled() => Err(astersql_pd_storage::StoreError::Cancelled),
            result = self.inner.read(read) => result,
        }
    }
    async fn transact(
        &self,
        compares: Vec<astersql_pd_storage::Compare>,
        writes: Vec<astersql_pd_storage::Mutation>,
    ) -> astersql_pd_storage::Result<astersql_pd_storage::Commit> {
        if self.cancel.is_cancelled() || self.leader.is_expired() {
            return Err(astersql_pd_storage::StoreError::Cancelled);
        }
        self.leader
            .transact(compares, writes)
            .await
            .map_err(|error| match error {
                MemberError::Store(error) => error,
                MemberError::Cancelled => astersql_pd_storage::StoreError::Cancelled,
                _ => astersql_pd_storage::StoreError::PermissionDenied,
            })
    }
    async fn transact_branches(
        &self,
        _: Vec<astersql_pd_storage::Compare>,
        _: Vec<astersql_pd_storage::Mutation>,
        _: Vec<astersql_pd_storage::Mutation>,
    ) -> astersql_pd_storage::Result<astersql_pd_storage::Commit> {
        Err(astersql_pd_storage::StoreError::PermissionDenied)
    }
    async fn transact_raw(
        &self,
        _: Vec<astersql_pd_storage::Compare>,
        _: Vec<astersql_pd_storage::RawTxnOp>,
        _: Vec<astersql_pd_storage::RawTxnOp>,
    ) -> astersql_pd_storage::Result<astersql_pd_storage::RawTxnResponse> {
        Err(astersql_pd_storage::StoreError::PermissionDenied)
    }
    async fn watch(
        &self,
        prefix: Vec<u8>,
        revision: astersql_pd_storage::Revision,
    ) -> astersql_pd_storage::Result<Box<dyn astersql_pd_storage::MetaWatch>> {
        self.inner.watch(prefix, revision).await
    }
    async fn grant(&self, _: i64) -> astersql_pd_storage::Result<astersql_pd_storage::Lease> {
        Err(astersql_pd_storage::StoreError::PermissionDenied)
    }
    async fn keep_alive(
        &self,
        _: astersql_pd_storage::LeaseId,
    ) -> astersql_pd_storage::Result<astersql_pd_storage::Lease> {
        Err(astersql_pd_storage::StoreError::PermissionDenied)
    }
    async fn revoke(&self, _: astersql_pd_storage::LeaseId) -> astersql_pd_storage::Result<()> {
        Err(astersql_pd_storage::StoreError::PermissionDenied)
    }
    async fn ttl(&self, lease: astersql_pd_storage::LeaseId) -> astersql_pd_storage::Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, _: astersql_pd_storage::Revision) -> astersql_pd_storage::Result<()> {
        Err(astersql_pd_storage::StoreError::PermissionDenied)
    }
}

#[async_trait::async_trait]
impl astersql_pd_server::bucket_heartbeat_adapter::BucketHeartbeatService for RoleServices {
    async fn bucket_heartbeat(
        &self,
        report: astersql_pd_statistics_buckets::BucketReport,
    ) -> Result<bool, astersql_pd_cluster::BucketRegionNotFound> {
        let Ok(term) = self.active().await else {
            return Ok(false);
        };
        term.cluster.bucket_heartbeat(report).await
    }
}
