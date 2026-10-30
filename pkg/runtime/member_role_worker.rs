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
    listening: std::sync::atomic::AtomicBool,
    admin: RwLock<Option<Arc<crate::member_admin::EtcdMemberAdmin>>>,
    diagnostics: RwLock<Option<Arc<crate::etcd_diagnostics::EtcdMemberDiagnostics>>>,
    follower: FollowerRead,
    store: Weak<dyn MetaStore>,
    term: RwLock<Option<Arc<Application>>>,
}
impl RoleServices {
    pub(crate) fn new(identity: NodeConfig, store: Arc<dyn MetaStore>) -> Self {
        Self {
            follower: FollowerRead(Arc::new(tokio::sync::Mutex::new(
                astersql_pd_server::sync_transport::SyncFollowerState {
                    cluster: astersql_pd_core::BasicCluster::new(),
                    history: astersql_pd_syncer::HistoryBuffer::new(10000)
                        .expect("nonzero history capacity"),
                    apply: astersql_pd_syncer::ClientApply::default(),
                    // Sync application persists its checkpoint as well as updating the cache.
                    // This nonauthoritative projection must never write the shared,
                    // leader-fenced metadata namespace.
                    storage: astersql_pd_storage::Storage::new(Arc::new(
                        astersql_pd_storage::MemoryStore::default(),
                    )),
                    paths: astersql_pd_keypath::ClusterPaths::new(identity.cluster_id),
                },
            ))),
            listening: std::sync::atomic::AtomicBool::new(false),
            diagnostics: RwLock::new(None),
            admin: RwLock::new(None),
            identity,
            store: Arc::downgrade(&store),
            term: RwLock::new(None),
        }
    }
    pub(crate) fn set_member_admin(
        &self,
        admin: Option<Arc<crate::member_admin::EtcdMemberAdmin>>,
    ) {
        *self.admin.write().unwrap() = admin;
    }
    pub(crate) fn set_diagnostics(
        &self,
        value: Option<Arc<crate::etcd_diagnostics::EtcdMemberDiagnostics>>,
    ) {
        *self.diagnostics.write().unwrap() = value;
    }
    pub(crate) fn set_listening(&self, value: bool) {
        self.listening
            .store(value, std::sync::atomic::Ordering::Release);
    }
    pub(crate) fn current(&self) -> Option<Arc<Application>> {
        self.term.read().unwrap().clone()
    }
    pub(crate) fn replace(&self, term: Option<Arc<Application>>) {
        *self.term.write().unwrap() = term;
    }
    async fn active(&self) -> Result<Arc<Application>, String> {
        crate::member_admin::ensure_admitted(
            self.store.upgrade().ok_or("server not started")?.as_ref(),
            self.identity.cluster_id,
            self.identity.member_id,
        )
        .await?;
        let term = self.current().ok_or("not leader")?;
        term.healthy().await?;
        Ok(term)
    }
}
#[async_trait::async_trait]
impl PdServices for RoleServices {
    async fn member_metadata(&self, id: u64) -> astersql_pd_server::http_member::MemberMetadata {
        let Some(store) = self.store.upgrade() else {
            return Default::default();
        };
        let cancel = astersql_pd_server::request_context::current()
            .map(|c| c.cancel)
            .unwrap_or_default();
        let info = astersql_pd_member::MemberInfo::new(
            store.clone(),
            cancel.clone(),
            self.identity.cluster_id,
        );
        let priority =
            astersql_pd_member::LeaderPriority::new(store, cancel, self.identity.cluster_id);
        astersql_pd_server::http_member::MemberMetadata {
            binary_version: String::from_utf8_lossy(
                &info.get_member_binary_version(id).await.unwrap_or_default(),
            )
            .into_owned(),
            deploy_path: String::from_utf8_lossy(
                &info.get_member_deploy_path(id).await.unwrap_or_default(),
            )
            .into_owned(),
            git_hash: String::from_utf8_lossy(
                &info.get_member_git_hash(id).await.unwrap_or_default(),
            )
            .into_owned(),
            leader_priority: priority
                .get_member_leader_priority(id)
                .await
                .unwrap_or_default(),
        }
    }
    async fn set_member_priority(&self, id: u64, priority: i64) -> Result<(), String> {
        let term = self.active().await?;
        let store = self.store.upgrade().ok_or("server not started")?;
        let cancel = astersql_pd_server::request_context::current()
            .map(|c| c.cancel)
            .unwrap_or_else(|| term.cancel.child_token());
        astersql_pd_member::LeaderPriority::new(store, cancel, self.identity.cluster_id)
            .set_registered_member_leader_priority(&term.leader, id, priority)
            .await
            .map_err(|e| e.to_string())
    }
    async fn remove_member(&self, id: u64) -> Result<(), String> {
        let term = self.active().await?;
        let admin = self.admin.read().unwrap().clone().ok_or(
            "dedicated member ownership is not configured; shared backend removal refused",
        )?;
        admin
            .remove(
                self.store.upgrade().ok_or("server not started")?,
                &term.leader,
                self.identity.cluster_id,
                id,
            )
            .await
    }
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
    fn follower_read_service(&self) -> Option<&dyn ClusterReadService> {
        Some(&self.follower)
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
    fn cluster_config_store(&self) -> Option<Arc<dyn MetaStore>> {
        let term = self.current()?;
        Some(Arc::new(TermStore::new(
            self.store.upgrade()?,
            term.leader.clone(),
            term.cancel.clone(),
        )))
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
    async fn external_timestamp(&self) -> Result<u64, String> {
        self.active().await?.cluster.external_timestamp().await
    }
    async fn set_external_timestamp(&self, timestamp: u64) -> Result<(), String> {
        self.active()
            .await?
            .cluster
            .set_external_timestamp(timestamp)
            .await
    }
    async fn report_min_resolved_ts(&self, store_id: u64, ts: u64) -> Result<(), String> {
        self.active()
            .await?
            .cluster
            .report_min_resolved_ts(store_id, ts)
            .await
    }
    async fn put_store(
        &self,
        store: astersql_pd_core::StoreMeta,
    ) -> Result<(), astersql_pd_server::grpc_store::StoreError> {
        let term = self
            .active()
            .await
            .map_err(astersql_pd_server::grpc_store::StoreError::Other)?;
        term.cluster.register_store(store).await
    }
    async fn stores(&self) -> Result<Vec<astersql_pd_core::StoreMeta>, String> {
        self.active().await?.cluster.registered_stores().await
    }
    async fn split_regions(
        &self,
        request: astersql_pd_server::grpc_split_regions::SplitRequest,
    ) -> Result<astersql_pd_server::grpc_split_regions::SplitResult, String> {
        self.active().await?.cluster.split_regions(request).await
    }
    async fn scatter(
        &self,
        request: astersql_pd_server::grpc_scatter::ScatterRequest,
    ) -> Result<
        astersql_pd_server::grpc_scatter::ScatterResult,
        astersql_pd_server::grpc_scatter::ScatterError,
    > {
        self.active()
            .await
            .map_err(astersql_pd_server::grpc_scatter::ScatterError::Other)?
            .cluster
            .scatter(request)
            .await
    }
    async fn operator(
        &self,
        id: u64,
    ) -> Result<Option<astersql_pd_server::grpc_scatter::OperatorResult>, String> {
        self.active().await?.cluster.operator(id).await
    }
    async fn local_consensus_probe(
        &self,
        probe: astersql_pd_server::http_health::LocalConsensusProbe,
    ) -> Result<(), String> {
        let diagnostics = self
            .diagnostics
            .read()
            .unwrap()
            .clone()
            .ok_or("local consensus diagnostics are not configured")?;
        diagnostics.check(probe).await
    }
    fn probe_listener_ready(&self) -> bool {
        self.listening.load(std::sync::atomic::Ordering::Acquire)
    }
    async fn probe_status(&self) -> Result<astersql_pd_server::http_health::ReadyStatus, String> {
        if !self.probe_listener_ready() {
            return Err("listener is not serving".into());
        }
        let store = self.store.upgrade().ok_or("server not started")?;
        let bootstrapped = astersql_pd_storage_endpoint::StorageEndpoint::new(store)
            .load_meta(&astersql_pd_keypath::ClusterPaths::new(
                self.identity.cluster_id,
            ))
            .await
            .map_err(|error| error.to_string())?
            .is_some();
        let loaded = if !bootstrapped {
            false
        } else if let Some(term) = self.current() {
            // Publication follows restore; serialize with bootstrap and retirement.
            term.cluster.regions_initialized().await
                && self
                    .current()
                    .is_some_and(|current| Arc::ptr_eq(&current, &term))
        } else {
            let state = self.follower.0.lock().await;
            state.apply.is_running() && !state.apply.full_syncing()
        };
        Ok(astersql_pd_server::http_health::ReadyStatus {
            region_loaded: loaded,
            is_bootstrapped: bootstrapped,
        })
    }
    async fn healthy(&self) -> Result<(), String> {
        self.active().await.map(|_| ())
    }
    async fn batch_split_version(&self) -> Result<Option<String>, String> {
        self.active().await?.cluster.batch_split_version().await
    }
    async fn split_ids(
        &self,
        region: astersql_pd_core::RegionMeta,
        count: u32,
        batch: bool,
        reason: i32,
    ) -> Result<
        astersql_pd_server::grpc_ask_split::SplitIds,
        astersql_pd_server::grpc_ask_split::SplitError,
    > {
        self.active()
            .await
            .map_err(astersql_pd_server::grpc_ask_split::SplitError::Other)?
            .cluster
            .split_ids(region, count, batch, reason)
            .await
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        self.active().await?.alloc_id().await
    }
    async fn alloc_ids(&self, count: u32) -> Result<(u64, u32), String> {
        self.active().await?.alloc_ids(count).await
    }
    async fn timestamp(&self, count: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        self.current()
            .ok_or_else(|| crate::tso::generation_error("requested pd is not leader of cluster"))?
            .timestamp(count)
            .await
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
    async fn query_regions(
        &self,
        keys: &[Vec<u8>],
        prev_keys: &[Vec<u8>],
        ids: &[u64],
        need_buckets: bool,
    ) -> Result<Option<astersql_pd_core::RegionQueryResult>, String> {
        self.active()
            .await?
            .cluster
            .query_regions(keys, prev_keys, ids, need_buckets)
            .await
    }

    async fn batch_scan_regions(
        &self,
        ranges: &[astersql_pd_core::KeyRange],
        options: astersql_pd_core::BatchScanRegionsOptions,
    ) -> Result<
        Result<Vec<Arc<astersql_pd_core::RegionInfo>>, astersql_pd_core::RegionNotAdjacent>,
        String,
    > {
        self.active()
            .await?
            .cluster
            .batch_scan_regions(ranges, options)
            .await
    }
    async fn store(&self, id: u64) -> Result<Option<Arc<astersql_pd_core::StoreInfo>>, String> {
        self.active().await?.cluster.store(id).await
    }
    async fn scan_regions(
        &self,
        start: &[u8],
        end: &[u8],
        limit: isize,
    ) -> Result<Vec<Arc<astersql_pd_core::RegionInfo>>, String> {
        self.active()
            .await?
            .cluster
            .scan_regions(start, end, limit)
            .await
    }
    async fn region_by_key(
        &self,
        key: &[u8],
        previous: bool,
    ) -> Result<Option<Arc<astersql_pd_core::RegionInfo>>, String> {
        self.active()
            .await?
            .cluster
            .region_by_key(key, previous)
            .await
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
                use prost::Message;
                let leader = astersql_pd_proto::pdpb::Member::decode(
                    snapshot
                        .entries
                        .iter()
                        .find(|e| e.key == key)
                        .unwrap()
                        .value
                        .as_slice(),
                )?;
                let endpoint = leader
                    .client_urls
                    .first()
                    .ok_or("leader has no client URL")?
                    .clone();
                let mut sync = astersql_pd_server::sync_transport::SyncFollower {
                    endpoint,
                    request: astersql_pd_server::sync_codec::SyncRequest {
                        header: Some(astersql_pd_server::sync_codec::SyncRequestHeader {
                            cluster_id: config.cluster_id,
                            ..Default::default()
                        }),
                        member: Some(astersql_pd_server::sync_codec::SyncMember {
                            name: config.name.clone(),
                            member_id: config.member_id,
                            client_urls: vec![config.client_url.clone()],
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    state: services.follower.0.clone(),
                };
                services.follower.0.lock().await.apply.reset_stream();
                let synchronize = async {
                    use astersql_pd_syncer::SyncAttempt;
                    loop {
                        let _ = sync.sync().await;
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                };
                tokio::select! { biased;
                    _=cancel.cancelled()=>{},
                    _=watch.next(&cancel)=>{},
                    _=synchronize=>{},
                }
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
    // Host shutdown can race with follower promotion after the election commit
    // but before prepare_term publishes services. Its typed cancellation is a
    // normal shutdown; unrelated backend/setup errors still propagate.
    let result = match result {
        Err(error)
            if cancel.is_cancelled()
                && error
                    .downcast_ref::<MemberError>()
                    .is_some_and(|e| matches!(e, MemberError::Cancelled)) =>
        {
            Ok(())
        }
        other => other,
    };
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

/// Follower cache reads never fall back to leader state or persistent metadata.
struct FollowerRead(Arc<tokio::sync::Mutex<astersql_pd_server::sync_transport::SyncFollowerState>>);
#[async_trait::async_trait]
impl ClusterReadService for FollowerRead {
    async fn query_regions(
        &self,
        keys: &[Vec<u8>],
        prev_keys: &[Vec<u8>],
        ids: &[u64],
        _need_buckets: bool,
    ) -> Result<Option<astersql_pd_core::RegionQueryResult>, String> {
        let state = self.0.lock().await;
        if !state.apply.is_running() {
            return Ok(None);
        }
        let keys: Vec<_> = keys.iter().map(Vec::as_slice).collect();
        let prev_keys: Vec<_> = prev_keys.iter().map(Vec::as_slice).collect();
        Ok(Some(
            state
                .cluster
                .regions
                .query_regions(&keys, &prev_keys, ids, false),
        ))
    }

    async fn batch_scan_regions(
        &self,
        ranges: &[astersql_pd_core::KeyRange],
        options: astersql_pd_core::BatchScanRegionsOptions,
    ) -> Result<
        Result<Vec<Arc<astersql_pd_core::RegionInfo>>, astersql_pd_core::RegionNotAdjacent>,
        String,
    > {
        let state = self.0.lock().await;
        Ok(if state.apply.is_running() {
            state.cluster.regions.batch_scan_regions(ranges, options)
        } else {
            Ok(vec![])
        })
    }
    async fn store(&self, _id: u64) -> Result<Option<Arc<astersql_pd_core::StoreInfo>>, String> {
        Err("follower store reads are not supported".into())
    }
    async fn region(&self, id: u64) -> Result<Option<Arc<astersql_pd_core::RegionInfo>>, String> {
        let state = self.0.lock().await;
        Ok(if state.apply.is_running() {
            state.cluster.regions.get_region(id)
        } else {
            None
        })
    }
    async fn scan_regions(
        &self,
        start: &[u8],
        end: &[u8],
        limit: isize,
    ) -> Result<Vec<Arc<astersql_pd_core::RegionInfo>>, String> {
        let state = self.0.lock().await;
        Ok(if state.apply.is_running() {
            state.cluster.regions.scan_regions(start, end, limit)
        } else {
            vec![]
        })
    }
    async fn region_by_key(
        &self,
        key: &[u8],
        previous: bool,
    ) -> Result<Option<Arc<astersql_pd_core::RegionInfo>>, String> {
        let state = self.0.lock().await;
        Ok(if state.apply.is_running() {
            state.cluster.regions.region_by_key(key, previous)
        } else {
            None
        })
    }
}
