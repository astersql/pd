// Copyright 2026 AsterSQL.
//! Independent scheduling leadership over the shared canonical cluster engine.
use crate::cluster_host::{ClusterHost, ClusterRole, SchedulePhase};
use astersql_pd_core::BasicCluster;
use astersql_pd_keypath::{ClusterPaths, MsParam};
use astersql_pd_mcs_discovery::{ServiceRegister, ServiceRegistryEntry};
use astersql_pd_mcs_scheduling_server::{Host, PdLeaderLookup, RemoteIds, Service};
use astersql_pd_mcs_server::{BaseServer, DelegateConnections};
use astersql_pd_mcs_utils::StoreWatcher;
use astersql_pd_member::{LeaderWatch, Leadership};
use astersql_pd_proto::schedulingpb::{Participant, scheduling_server::SchedulingServer};
use astersql_pd_schedule_config::{ProviderSnapshot, ScheduleDefaults};
use astersql_pd_server::{Identity, PdServices};
use astersql_pd_server_config::{ConfigDefaultsContext, PersistedConfig};
use astersql_pd_storage::{Compare, Consistency, Event, MetaStore, Read};
use futures_util::{FutureExt, future::FusedFuture};
use prost::Message;
use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{runtime::Handle, sync::Notify, task::JoinHandle};
use tokio_util::sync::CancellationToken;
/// The embedding process supplies executor, backend, transport and metrics scope.
pub struct SchedulingHostConfig {
    pub runtime: Handle,
    pub cancel: CancellationToken,
    pub store: Arc<dyn MetaStore>,
    pub cluster_id: u64,
    pub entry: ServiceRegistryEntry,
    pub delegates: Arc<DelegateConnections>,
    pub registry: Arc<prometheus::Registry>,
    pub pd_leader_lookup: PdLeaderLookup,
}
struct State {
    cluster_id: u64,
    cancel: CancellationToken,
    reload: Notify,
    member_reload: Arc<Notify>,
    term: RwLock<Option<Arc<Term>>>,
}
impl Host for State {
    fn cluster_id(&self) -> u64 {
        self.cluster_id
    }
    fn cluster(&self) -> Option<Arc<dyn PdServices>> {
        if self.cancel.is_cancelled() {
            return None;
        }
        self.term
            .read()
            .unwrap()
            .as_ref()
            .filter(|term| !term.cancel.is_cancelled() && !term.leader.is_expired())
            .cloned()
            .map(|term| term as Arc<dyn PdServices>)
    }
    fn force_load_stores(&self) {
        self.reload.notify_one();
    }
}
struct Term {
    identity: Identity,
    cluster: Arc<ClusterHost>,
    leader: Arc<Leadership>,
    cancel: CancellationToken,
    ids: Arc<RemoteIds>,
}
#[async_trait::async_trait]
impl PdServices for Term {
    fn identity(&self) -> &Identity {
        &self.identity
    }
    async fn healthy(&self) -> Result<(), String> {
        if self.cancel.is_cancelled() {
            return Err("server not started".into());
        }
        self.leader.validate().await.map_err(|e| e.to_string())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        self.ids
            .alloc_count(1)
            .await
            .map(|r| r.0)
            .map_err(|e| e.to_string())
    }
    async fn alloc_ids(&self, count: u32) -> Result<(u64, u32), String> {
        self.ids.alloc_count(count).await.map_err(|e| e.to_string())
    }
    async fn timestamp(&self, _count: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        Err("TSO is not a scheduling operation".into())
    }
    fn region_heartbeat_service(
        &self,
    ) -> Option<&dyn astersql_pd_server::region_heartbeat_adapter::RegionHeartbeatService> {
        Some(self.cluster.as_ref())
    }
    fn store_heartbeat_service(
        &self,
    ) -> Option<&dyn astersql_pd_server::store_heartbeat_adapter::StoreHeartbeatService> {
        Some(self.cluster.as_ref())
    }
    fn bucket_heartbeat_service(
        &self,
    ) -> Option<&dyn astersql_pd_server::bucket_heartbeat_adapter::BucketHeartbeatService> {
        Some(self.cluster.as_ref())
    }
    fn cluster_read_service(&self) -> Option<&dyn astersql_pd_server::ClusterReadService> {
        Some(self.cluster.as_ref())
    }
    async fn split_regions(
        &self,
        request: astersql_pd_server::grpc_split_regions::SplitRequest,
    ) -> Result<astersql_pd_server::grpc_split_regions::SplitResult, String> {
        self.cluster.split_regions(request).await
    }
    async fn scatter(
        &self,
        request: astersql_pd_server::grpc_scatter::ScatterRequest,
    ) -> Result<
        astersql_pd_server::grpc_scatter::ScatterResult,
        astersql_pd_server::grpc_scatter::ScatterError,
    > {
        self.cluster.scatter(request).await
    }
    async fn operator(
        &self,
        id: u64,
    ) -> Result<Option<astersql_pd_server::grpc_scatter::OperatorResult>, String> {
        self.cluster.operator(id).await
    }
    async fn split_ids(
        &self,
        region: astersql_pd_core::RegionMeta,
        count: u32,
        _batch: bool,
        _reason: i32,
    ) -> Result<
        astersql_pd_server::grpc_ask_split::SplitIds,
        astersql_pd_server::grpc_ask_split::SplitError,
    > {
        self.cluster.validate_scheduling_split(&region).await?;
        self.ids
            .split_ids(region.peers.len(), count)
            .await
            .map_err(astersql_pd_server::grpc_ask_split::SplitError::Forwarded)
    }
}
/// Owns the MCS listener, primary lease and joined domain resources. This never
/// constructs a PdNode, installs process signals, or creates a private runtime.
pub struct SchedulingHost {
    state: Arc<State>,
    listener: BaseServer,
    registry: Option<ServiceRegister>,
    worker: Option<JoinHandle<()>>,
    membership: Option<JoinHandle<()>>,
}
impl SchedulingHost {
    pub async fn start<L>(config: SchedulingHostConfig, listener: L) -> Result<Self, String>
    where
        L: axum::serve::Listener<Addr = SocketAddr> + Send + 'static,
    {
        Self::start_with_log_file(config, listener, String::new()).await
    }
    /// Register common diagnostics with the log path owned by the embedding host.
    pub async fn start_with_log_file<L>(
        config: SchedulingHostConfig,
        listener: L,
        log_file: String,
    ) -> Result<Self, String>
    where
        L: axum::serve::Listener<Addr = SocketAddr> + Send + 'static,
    {
        Self::start_with_primary_lease(
            config,
            listener,
            log_file,
            astersql_pd_mcs_server::DEFAULT_LEASE,
        )
        .await
    }
    /// Use the caller's primary lease while keeping registry leases independent.
    pub async fn start_with_primary_lease<L>(
        config: SchedulingHostConfig,
        listener: L,
        log_file: String,
        primary_lease: i64,
    ) -> Result<Self, String>
    where
        L: axum::serve::Listener<Addr = SocketAddr> + Send + 'static,
    {
        let cancel = config.cancel.child_token();
        let state = Arc::new(State {
            cluster_id: config.cluster_id,
            cancel: cancel.clone(),
            reload: Notify::new(),
            member_reload: Arc::new(Notify::new()),
            term: RwLock::new(None),
        });
        let service = Service::new(state.clone(), cancel.clone());
        let diagnostics_runtime = config.runtime.clone();
        let mut listener = BaseServer::start(
            config.runtime.clone(),
            &cancel,
            listener,
            move |cancel| {
                Ok(
                    astersql_pd_mcs_server::auxiliary_routes(diagnostics_runtime, cancel, log_file)
                        .add_service(SchedulingServer::new(service))
                        .into_axum_router()
                        .fallback(astersql_pd_mcs_server::unregistered_service),
                )
            },
            vec![],
        )
        .await?;
        let registry = match ServiceRegister::start(
            config.store.clone(),
            config.runtime.clone(),
            cancel.clone(),
            config.cluster_id,
            "scheduling",
            &config.entry.service_addr,
            config.entry.serialize().map_err(|e| e.to_string())?,
            5,
        )
        .await
        {
            Ok(registry) => registry,
            Err(error) => {
                cancel.cancel();
                let _ = listener.shutdown().await;
                return Err(error.to_string());
            }
        };
        let membership_state = state.clone();
        let lookup = config.pd_leader_lookup.clone();
        let membership = config.runtime.spawn(async move {
            watch_membership(membership_state, lookup).await;
        });
        let task_state = state.clone();
        let runtime = config.runtime.clone();
        let worker = runtime.spawn(async move {
            run(config, task_state, primary_lease).await;
        });
        Ok(Self {
            state,
            listener,
            registry: Some(registry),
            worker: Some(worker),
            membership: Some(membership),
        })
    }
    pub async fn shutdown(mut self) -> Result<(), String> {
        self.state.cancel.cancel();
        let result = self.listener.shutdown().await;
        if let Some(worker) = self.worker.take() {
            worker.await.map_err(|e| e.to_string())?;
        }
        if let Some(membership) = self.membership.take() {
            membership.await.map_err(|e| e.to_string())?;
        }
        if let Some(registry) = self.registry.take() {
            registry.shutdown().await.map_err(|e| e.to_string())?;
        }
        result
    }
}
impl Drop for SchedulingHost {
    fn drop(&mut self) {
        self.state.cancel.cancel();
    }
}
async fn pause(cancel: &CancellationToken) {
    tokio::select! {_=cancel.cancelled()=>{},_=tokio::time::sleep(Duration::from_millis(100))=>{}}
}
async fn open(
    config: &SchedulingHostConfig,
    leader: Arc<Leadership>,
    cancel: CancellationToken,
    member_reload: Arc<Notify>,
) -> Result<
    (
        Arc<Term>,
        StoreWatcher,
        astersql_pd_etcdutil::ReconciledWatch,
    ),
    String,
> {
    let paths = ClusterPaths::new(config.cluster_id);
    // Domain persistence is a term-local projection, matching Go MemoryKV.
    // Election, registry and observations retain the real shared backend.
    let domain: Arc<dyn MetaStore> = Arc::new(astersql_pd_storage::MemoryStore::default());
    // Seed only PD-owned policy inputs into the local storage projection.
    // Canonical managers build their indices from these snapshots; all ensuing
    // scheduler configuration/rule housekeeping remains local to this term.
    for prefix in [
        paths.rule_common_path_prefix(),
        paths.region_label_path_prefix(),
        paths.affinity_groups_prefix(),
        paths.scheduler_config_path_prefix(),
    ] {
        let snapshot = astersql_pd_etcdutil::SnapshotLoader::new(prefix.into_bytes(), true, 128)
            .load(config.store.as_ref(), &cancel)
            .await
            .map_err(|e| e.to_string())?;
        let writes = snapshot
            .entries
            .into_iter()
            .map(|entry| astersql_pd_storage::Mutation::put(&entry.key, &entry.value))
            .collect::<Vec<_>>();
        if !writes.is_empty() {
            domain
                .transact(vec![], writes)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    let (config_snapshot, config_watch) = astersql_pd_etcdutil::ReconciledWatch::load(
        config.store.clone(),
        paths.config_path().into_bytes(),
        false,
        128,
        &cancel,
    )
    .await
    .map_err(|e| e.to_string())?;
    // A Scheduling primary owns an independent in-memory cache. Go starts it
    // before PD bootstrap and lets the metadata watcher populate stores later.
    let mut persisted = PersistedConfig::default();
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let found = if let Some(entry) = config_snapshot.entries.first() {
        persisted
            .apply_document(
                &entry.value,
                ConfigDefaultsContext {
                    hostname: &config.entry.name,
                    cwd: &cwd,
                },
                &mut ScheduleDefaults::default(),
            )
            .map_err(|e| e.to_string())?;
        true
    } else {
        false
    };
    if !found {
        persisted
            .config
            .adjust(
                &Default::default(),
                ConfigDefaultsContext {
                    hostname: &config.entry.name,
                    cwd: &cwd,
                },
                true,
                &mut ScheduleDefaults::default(),
            )
            .map_err(|e| e.to_string())?;
    }
    let mut provider = ProviderSnapshot::default();
    crate::cluster_restore::apply_persisted_config(&persisted, &mut provider)
        .map_err(|e| e.to_string())?;
    let cluster = BasicCluster::new();
    let watch = StoreWatcher::load(config.store.clone(), paths, cluster.stores.clone(), &cancel)
        .await
        .map_err(|e| e.to_string())?;
    // The shared constructor retains its classic allocator capability, but this
    // role never rebases or consumes it: split IDs go through PD's public RPC.
    let allocator = Arc::new(
        astersql_pd_id::Allocator::new(config.store.clone(), leader.clone(), cancel.clone(), 0)
            .map_err(|e| e.to_string())?,
    );
    let member_id = astersql_pd_memberutil::generate_unique_id(&config.entry.service_addr);
    let ids = Arc::new(RemoteIds::new(
        config.cluster_id,
        config.delegates.clone(),
        cancel.clone(),
        member_reload,
        config.runtime.clone(),
    ));
    let engine = Arc::new(
        ClusterHost::new_with_role(
            cluster,
            provider,
            Some(persisted),
            domain,
            config.cluster_id,
            member_id,
            cancel.clone(),
            &config.registry,
            leader.clone(),
            allocator,
            None,
            None,
            Arc::new(astersql_pd_unsaferecovery::RecoverySteps::new(0)),
            ClusterRole::Scheduling,
            Some(ids.clone()),
        )
        .await
        .map_err(|e| e.to_string())?,
    );
    let mut status = engine.watch_schedule();
    let ready=async {
        loop {
            match status.borrow().phase {
                SchedulePhase::Preparing|SchedulePhase::Running=>return Ok(()),
                SchedulePhase::Stopped=>return Err(engine.schedule_error().unwrap_or_else(||"scheduling initialization stopped".into())),
                SchedulePhase::Starting=>{},
            }
            tokio::select! {biased;_=cancel.cancelled()=>return Err("scheduling initialization canceled".into()),result=status.changed()=>{result.map_err(|e|e.to_string())?;}}
        }
    }.await;
    if let Err(error) = ready {
        let _ = engine.shutdown().await;
        return Err(error);
    }
    let term = Arc::new(Term {
        identity: Identity {
            cluster_id: config.cluster_id,
            member_id,
            name: config.entry.name.clone(),
            client_url: config.entry.service_addr.clone(),
            peer_url: String::new(),
        },
        cluster: engine,
        leader,
        cancel: cancel.clone(),
        ids,
    });
    Ok((term, watch, config_watch))
}
async fn run(config: SchedulingHostConfig, state: Arc<State>, primary_lease: i64) {
    let paths = ClusterPaths::new(config.cluster_id);
    let param = MsParam {
        service_name: "scheduling".into(),
        group_id: 0,
    };
    let key = paths.election_path(Some(&param)).into_bytes();
    let value = Participant {
        name: config.entry.service_addr.clone(),
        id: astersql_pd_memberutil::generate_unique_id(&config.entry.service_addr),
        listen_urls: vec![config.entry.service_addr.clone()],
    }
    .encode_to_vec();
    while !state.cancel.is_cancelled() {
        let attempt = async {
            let snapshot = config
                .store
                .read(Read::range(&key, b"", Consistency::Linearizable))
                .await
                .map_err(|e| e.to_string())?;
            if !snapshot.entries.is_empty() {
                let mut watch =
                    LeaderWatch::new(config.store.clone(), key.clone(), snapshot.revision + 1)
                        .map_err(|e| e.to_string())?;
                while let Ok(event) = watch.next(&state.cancel).await {
                    if matches!(event, Event::Delete { .. }) {
                        break;
                    }
                }
                return Ok::<(), String>(());
            }
            let expected =
                astersql_pd_mcs_utils::expected_primary(config.store.as_ref(), &paths, &param)
                    .await?;
            let guards = match &expected {
                Some(row) if !row.value.is_empty() => {
                    if row.value != config.entry.service_addr.as_bytes() {
                        return Ok(());
                    }
                    vec![Compare::Value(
                        paths.expected_primary_path(&param).into_bytes(),
                        row.value.clone(),
                    )]
                }
                _ => vec![],
            };
            let cancel = state.cancel.child_token();
            let leader = Arc::new(
                Leadership::campaign_with_key(
                    config.store.clone(),
                    cancel.clone(),
                    config.cluster_id,
                    key.clone(),
                    value.clone(),
                    primary_lease,
                    guards,
                )
                .await
                .map_err(|e| e.to_string())?,
            );
            let keep = leader.keep(&config.runtime, cancel.clone());
            let keep_join = keep.join().fuse();
            tokio::pin!(keep_join);
            let opened = tokio::select! {biased;_=state.cancel.cancelled()=>None,_=&mut keep_join=>None,result=open(&config,leader.clone(),cancel.clone(),state.member_reload.clone())=>match result {Ok(opened)=>Some(opened),Err(error)=>{tracing::warn!(component="scheduling",%error,"primary initialization failed");None}}};
            if let Some((term, mut watch, config_watch)) = opened {
                let config_worker =
                    config
                        .runtime
                        .spawn(crate::cluster_host::scheduling_config_watch::run(
                            term.cluster.clone(),
                            config_watch,
                            cancel.clone(),
                        ));
                if let Some(row) = &expected {
                    let _ = astersql_pd_mcs_utils::clear_expected_primary(
                        config.store.as_ref(),
                        paths.expected_primary_path(&param).into_bytes(),
                        row,
                    )
                    .await;
                }
                *state.term.write().unwrap() = Some(term.clone());
                let mut status = term.cluster.watch_schedule();
                loop {
                    tokio::select! {biased;
                        _=state.cancel.cancelled()=>break,
                        _=&mut keep_join=>break,
                        changed=status.changed()=>{if changed.is_err() || status.borrow().phase == SchedulePhase::Stopped {break;}},
                        _=state.reload.notified()=>{if watch.reload(&cancel).await.is_err() {pause(&cancel).await;}},
                        result=watch.advance(&cancel)=>{if result.is_err() {pause(&cancel).await;}},
                    }
                }
                state.term.write().unwrap().take();
                cancel.cancel();
                let _ = config_worker.await;
                if let Err(error) = term.cluster.shutdown().await {
                    tracing::warn!(component="scheduling",%error,"domain shutdown failed");
                }
            }
            cancel.cancel();
            if !keep_join.is_terminated() {
                let _ = keep_join.await;
            }
            let _ = tokio::time::timeout(Duration::from_secs(5), leader.resign()).await;
            Ok(())
        };
        if let Err(error) = attempt.await {
            if !state.cancel.is_cancelled() {
                tracing::warn!(component="scheduling",%error,"primary attempt failed");
            }
        }
        pause(&state.cancel).await;
    }
}

async fn watch_membership(state: Arc<State>, lookup: PdLeaderLookup) {
    let mut ticker = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(60),
        Duration::from_secs(60),
    );
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {biased;_=state.cancel.cancelled()=>return,_=ticker.tick()=>{},_=state.member_reload.notified()=>{}}
        let term = state.term.read().unwrap().clone();
        if let Some(term) =
            term.filter(|term| !term.cancel.is_cancelled() && !term.leader.is_expired())
        {
            if let Err(error) = term.ids.refresh(&lookup).await {
                tracing::warn!(component="scheduling",%error,"PD client membership refresh failed");
            }
        }
    }
}

#[cfg(test)]
#[path = "embed_scheduling_test.rs"]
mod tests;

#[cfg(test)]
#[path = "embed_scheduling_config_test.rs"]
mod config_tests;
