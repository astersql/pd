// Copyright 2026 AsterSQL.
//! Shared canonical cluster state owned by the serving scope.
use astersql_pd_cluster::{
    HeartbeatEffectsState, HeartbeatRunners, StatePipeline, StateSnapshot, StoreConfigWorker,
    StoreStateWorker, StoreStatsWorker, check_store_states, refresh_store_stats, sync_store_config,
};
use astersql_pd_core::{BasicCluster, NodeState, RegionInfo, StoreStats};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_ratelimit::ConcurrentRunner;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_server::sync_transport::RegionSyncer;
use astersql_pd_server::{region_heartbeat_adapter::*, store_heartbeat_adapter::*};
use astersql_pd_statistics::{HotStat, LabelStatistics, RegionStatistics};
use astersql_pd_storage::MetaStore;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_syncer::{Downstreams, HistoryBuffer};
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

type State = Mutex<HeartbeatEffectsState<ProviderSnapshot<()>>>;
struct Resources {
    leader: Arc<astersql_pd_member::Leadership>,
    allocator: Arc<astersql_pd_id::Allocator>,
    schedule_observations: std::collections::BTreeSet<u64>,
    state: Arc<State>,
    hot: HotStat,
    runners: [ConcurrentRunner; 3],
    storage: Arc<StorageEndpoint>,
    paths: ClusterPaths,
    downstreams: Downstreams,
    syncer: Arc<RegionSyncer>,
    persisted_config: astersql_pd_server_config::PersistedConfig,
}
/// One cache and pipeline shared by all adapters and host readers.
/// The node creates workers on its supplied executor and joins them on shutdown.
/// Read handles retain no storage or workers after shutdown.
pub struct ClusterHost {
    #[cfg(test)]
    state: Weak<State>,
    stores: astersql_pd_core::StoresInfo,
    syncer: Weak<RegionSyncer>,
    restored_config: Option<astersql_pd_server_config::PersistedConfig>,
    resources: Arc<AsyncMutex<Option<Resources>>>,
    store_worker: AsyncMutex<Option<StoreStateWorker>>,
    stats_worker: AsyncMutex<Option<StoreStatsWorker>>,
    config_worker: AsyncMutex<Option<StoreConfigWorker>>,
    metrics_worker: AsyncMutex<Option<astersql_pd_cluster::ClusterMetricsWorker>>,
    schedule_status: tokio::sync::watch::Receiver<ScheduleStatus>,
    schedule_worker: AsyncMutex<Option<schedule_worker::ScheduleWorker>>,
    schedule_routing: astersql_pd_schedule_hbstream::routing::RoutingHandle,
    schedule_error: Arc<Mutex<Option<String>>>,
    schedule_initialized: Arc<std::sync::atomic::AtomicBool>,
    resolved_worker: AsyncMutex<Option<astersql_pd_cluster::ResolvedTsWorker>>,
    min_resolved_ts: Arc<std::sync::atomic::AtomicU64>,
    resolved_error: Arc<Mutex<Option<String>>>,
    store_config_error: Arc<Mutex<Option<String>>>,
    store_check_errors: Arc<Mutex<Vec<(u64, String)>>>,
    cancel: CancellationToken,
}
impl ClusterHost {
    pub(crate) async fn new(
        cluster: BasicCluster,
        config: ProviderSnapshot<()>,
        restored_config: Option<astersql_pd_server_config::PersistedConfig>,
        store: Arc<dyn MetaStore>,
        cluster_id: u64,
        cancel: CancellationToken,
        registry: &prometheus::Registry,
        leader: Arc<astersql_pd_member::Leadership>,
        allocator: Arc<astersql_pd_id::Allocator>,
    ) -> Result<Self, crate::NodeError> {
        let scheduling_cluster = cluster.clone();
        let stores = cluster.stores.clone();
        let mut persisted_config = restored_config.clone().unwrap_or_default();
        if restored_config.is_none() {
            // Persist the adjusted dynamic defaults, not raw zero-valued sections.
            // Host identity fields are excluded by PersistedConfig::persist.
            persisted_config.config.adjust(
                &Default::default(),
                astersql_pd_server_config::ConfigDefaultsContext {
                    hostname: "",
                    cwd: &std::env::current_dir()?,
                },
                false,
                &mut astersql_pd_schedule_config::ScheduleDefaults::default(),
            )?;
        }
        persisted_config.config.schedule = config.schedule.clone();
        persisted_config.config.replication = config.replication.clone();
        persisted_config.store = config.store.clone();
        let metrics_store = store.clone();
        let storage = Arc::new(StorageEndpoint::new(store));
        let paths = ClusterPaths::new(cluster_id);
        let resolved_error = Arc::new(Mutex::new(None));
        let initial_resolved = match storage.load_min_resolved_ts(&paths).await {
            Ok(value) => value,
            Err(error) => {
                *resolved_error.lock().unwrap() = Some(error.to_string());
                0
            }
        };
        let min_resolved_ts = Arc::new(std::sync::atomic::AtomicU64::new(initial_resolved));
        let resolved_interval = Arc::new(std::sync::atomic::AtomicU64::new(
            persisted_config
                .config
                .pd_server
                .min_resolved_ts_persistence_interval
                .as_nanos()
                .max(0) as u64,
        ));
        let mut rules = RuleManager::new(storage.clone(), paths);
        if config.replication.enable_placement_rules {
            let stores = cluster
                .stores
                .get_stores()
                .into_iter()
                .map(|s| s.as_ref().clone())
                .collect::<Vec<_>>();
            rules
                .initialize(
                    config.replication.max_replicas as i64,
                    config.replication.location_labels.as_slice(),
                    &config.replication.isolation_level,
                    false,
                    &config,
                    &stores,
                )
                .await?;
        }
        let hot = HotStat::new(&tokio::runtime::Handle::current(), registry, 128)?;
        let mut runners = std::array::from_fn(|_| ConcurrentRunner::new(None));
        for runner in &mut runners {
            runner.start(&cancel);
        }
        let state = Arc::new(Mutex::new(HeartbeatEffectsState {
            cluster,
            region_stats: Some(RegionStatistics::new()),
            label_stats: LabelStatistics::new(),
            config,
            rules,
            affinity: None,
        }));
        let syncer = Arc::new(RegionSyncer::for_heartbeat_state(
            tokio::runtime::Handle::current(),
            state.clone(),
            HistoryBuffer::new(100)?,
            std::time::Duration::from_secs(10),
        ));
        #[cfg(test)]
        let state_weak = Arc::downgrade(&state);
        let syncer_weak = Arc::downgrade(&syncer);
        let scheduling_configuration =
            serde_json::to_value(persisted_config.persistence_snapshot())
                .map_err(|e| e.to_string())?;
        let resources = Arc::new(AsyncMutex::new(Some(Resources {
            leader,
            allocator,
            schedule_observations: Default::default(),
            state: state.clone(),
            hot,
            runners,
            storage,
            paths,
            persisted_config,
            downstreams: syncer.downstreams(),
            syncer,
        })));
        let worker_resources = resources.clone();
        let worker_cancel = cancel.clone();
        let start = tokio::time::Instant::now();
        let count = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let total = count.clone();
        let preparation = Arc::new(astersql_pd_schedule::PrepareChecker::new(
            std::time::Duration::ZERO,
            move || {
                Ok::<i64, std::convert::Infallible>(
                    total.load(std::sync::atomic::Ordering::Relaxed),
                )
            },
        ));
        let store_check_errors = Arc::new(Mutex::new(Vec::new()));
        let worker_errors = store_check_errors.clone();
        let store_worker = StoreStateWorker::start(
            &tokio::runtime::Handle::current(),
            &cancel,
            std::time::Duration::from_secs(10),
            move || {
                let resources = worker_resources.clone();
                let cancel = worker_cancel.clone();
                let errors = worker_errors.clone();
                let preparation = preparation.clone();
                let count = count.clone();
                async move {
                    let guard = resources.lock().await;
                    let Some(resources) = guard.as_ref() else {
                        return;
                    };
                    if cancel.is_cancelled() || resources.leader.validate().await.is_err() {
                        return;
                    }
                    let prepared = {
                        let state = resources.state.lock().unwrap();
                        count.store(
                            state.cluster.regions.total_region_count() as i64,
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        preparation.check(&state.cluster, start.elapsed())
                    };
                    let result = check_store_states(
                        &resources.state,
                        &resources.storage,
                        resources.paths,
                        now().1 as i128,
                        prepared,
                    )
                    .await;
                    *errors.lock().unwrap() = result;
                    let state = resources.state.lock().unwrap();
                    for store in state.cluster.stores.get_stores() {
                        if store.is_removed() {
                            resources
                                .hot
                                .stores_stats
                                .remove_rolling_store_stats(store.id());
                        }
                    }
                }
            },
        );
        let stats_resources = resources.clone();
        let stats_cancel = cancel.clone();
        let stats_worker =
            StoreStatsWorker::start(&tokio::runtime::Handle::current(), &cancel, move || {
                let resources = stats_resources.clone();
                let cancel = stats_cancel.clone();
                async move {
                    let guard = resources.lock().await;
                    let Some(resources) = guard.as_ref() else {
                        return;
                    };
                    if cancel.is_cancelled() || resources.leader.validate().await.is_err() {
                        return;
                    }
                    refresh_store_stats(&resources.state.lock().unwrap().cluster);
                }
            });
        let config_resources = resources.clone();
        let store_config_error = Arc::new(Mutex::new(None));
        let config_error = store_config_error.clone();
        let candidates = Arc::new(AsyncMutex::new(Vec::new()));
        let config_worker =
            StoreConfigWorker::start(&tokio::runtime::Handle::current(), &cancel, move |cancel| {
                let resources = config_resources.clone();
                let candidates = candidates.clone();
                let error = config_error.clone();
                async move {
                    let mut guard = resources.lock().await;
                    let Some(resources) = guard.as_mut() else {
                        return;
                    };
                    let mut candidates = candidates.lock().await;
                    let result = resources
                        .sync_store_configuration(&mut candidates, &cancel)
                        .await;
                    *error.lock().unwrap() = result.err().map(|e| e.to_string());
                }
            });
        let metrics_worker = astersql_pd_cluster::ClusterMetricsWorker::start(
            &tokio::runtime::Handle::current(),
            &cancel,
            registry,
            move || {
                let store = metrics_store.clone();
                async move {
                    use astersql_pd_storage::{Consistency, Read};
                    use prost::Message;
                    let prefix = format!("/pd/{cluster_id}/rust/members/");
                    let snapshot = store
                        .read(Read::prefix(prefix.as_bytes(), Consistency::Linearizable))
                        .await
                        .map_err(|e| e.to_string())?;
                    snapshot
                        .entries
                        .into_iter()
                        .map(|entry| {
                            let member =
                                astersql_pd_proto::pdpb::Member::decode(entry.value.as_slice())
                                    .map_err(|e| e.to_string())?;
                            Ok(astersql_pd_cluster::HealthMember {
                                name: member.name,
                                client_urls: member.client_urls,
                            })
                        })
                        .collect()
                }
            },
        )?;
        let worker_resources = resources.clone();
        let current = min_resolved_ts.clone();
        let error = resolved_error.clone();
        let configured = resolved_interval.clone();
        let worker_cancel = cancel.clone();
        let resolved_worker = astersql_pd_cluster::ResolvedTsWorker::start(
            &tokio::runtime::Handle::current(),
            &cancel,
            move || {
                std::time::Duration::from_nanos(
                    configured.load(std::sync::atomic::Ordering::Relaxed),
                )
            },
            move || {
                let resources = worker_resources.clone();
                let current = current.clone();
                let error = error.clone();
                let cancel = worker_cancel.clone();
                async move {
                    let guard = resources.lock().await;
                    let Some(resources) = guard.as_ref() else {
                        return;
                    };
                    if cancel.is_cancelled() {
                        return;
                    }
                    if let Err(e) = resources.leader.validate().await {
                        *error.lock().unwrap() = Some(e.to_string());
                        return;
                    }
                    let candidate = {
                        let state = resources.state.lock().unwrap();
                        astersql_pd_cluster::min_resolved_ts_candidate(
                            &state.cluster,
                            state.config.replication.max_replicas as usize,
                        )
                    };
                    *error.lock().unwrap() = astersql_pd_cluster::refresh_min_resolved_ts(
                        candidate,
                        &current,
                        &resources.storage,
                        &resources.paths,
                    )
                    .await
                    .err()
                    .map(|e| e.to_string());
                }
            },
        );
        let schedule_error = Arc::new(Mutex::new(None));
        let schedule_initialized = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (schedule_routing, schedule_worker, schedule_status) =
            schedule_worker::ScheduleWorker::start(
                resources.clone(),
                scheduling_cluster,
                scheduling_configuration,
                cancel.clone(),
                schedule_error.clone(),
                schedule_initialized.clone(),
                cluster_id,
            );
        Ok(Self {
            stores,
            schedule_status,
            schedule_worker: AsyncMutex::new(Some(schedule_worker)),
            schedule_routing,
            schedule_error,
            schedule_initialized,
            resolved_worker: AsyncMutex::new(Some(resolved_worker)),
            min_resolved_ts,
            resolved_error,
            syncer: syncer_weak,
            #[cfg(test)]
            state: state_weak,
            restored_config,
            resources,
            store_worker: AsyncMutex::new(Some(store_worker)),
            stats_worker: AsyncMutex::new(Some(stats_worker)),
            config_worker: AsyncMutex::new(Some(config_worker)),
            metrics_worker: AsyncMutex::new(Some(metrics_worker)),
            store_config_error,
            store_check_errors,
            cancel,
        })
    }

    /// Subscribe to term progress without retaining workers or persistence resources.
    pub fn watch_schedule(&self) -> tokio::sync::watch::Receiver<ScheduleStatus> {
        self.schedule_status.clone()
    }
    /// True only after the preparation gate and scheduler recovery finish.
    pub fn schedule_initialized(&self) -> bool {
        self.schedule_initialized
            .load(std::sync::atomic::Ordering::Acquire)
    }
    /// Latest scheduler restore, execution or leadership failure.
    pub fn schedule_error(&self) -> Option<String> {
        self.schedule_error.lock().unwrap().clone()
    }
    /// Current monotonic minimum, recovered from the shared storage at startup.
    pub fn min_resolved_ts(&self) -> u64 {
        self.min_resolved_ts
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Latest resolved timestamp restore, leadership or persistence failure.
    pub fn resolved_ts_error(&self) -> Option<String> {
        self.resolved_error.lock().unwrap().clone()
    }
    /// Latest synchronization failure; the next periodic tick retries it.
    pub fn store_config_error(&self) -> Option<String> {
        self.store_config_error.lock().unwrap().clone()
    }
    /// Current configuration from the same generation read by cluster workers.
    pub async fn store_config(&self) -> Option<astersql_pd_schedule_config::StoreConfig> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return None;
        }
        guard
            .as_ref()
            .map(|r| r.state.lock().unwrap().config.store.clone())
    }
    /// Failures from the latest store check; subsequent ticks retry failed stores.
    pub fn store_state_check_errors(&self) -> Vec<(u64, String)> {
        self.store_check_errors.lock().unwrap().clone()
    }
    pub(crate) fn region_sync_handle(&self) -> Option<Arc<RegionSyncer>> {
        self.syncer.upgrade()
    }
    /// Dynamic configuration recovered before serving this node.
    pub fn restored_config(&self) -> Option<&astersql_pd_server_config::PersistedConfig> {
        self.restored_config.as_ref()
    }
    /// Reads the same cache that accepted heartbeats. Cancellation rejects new reads.
    pub async fn snapshot(&self) -> Result<StateSnapshot, crate::NodeError> {
        let resources = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let resources = resources.as_ref().ok_or("node stopped")?;
        Ok(resources.pipeline().snapshot().await?)
    }
    pub(crate) async fn shutdown(&self) -> Result<(), crate::NodeError> {
        self.cancel.cancel();
        let schedule_error = if let Some(worker) = self.schedule_worker.lock().await.take() {
            worker.stop().await.err()
        } else {
            None
        };
        let worker_error = if let Some(worker) = self.store_worker.lock().await.take() {
            worker.stop().await.err()
        } else {
            None
        };
        let stats_error = if let Some(worker) = self.stats_worker.lock().await.take() {
            worker.stop().await.err()
        } else {
            None
        };
        let config_error = if let Some(worker) = self.config_worker.lock().await.take() {
            worker.stop().await.err()
        } else {
            None
        };
        let metrics_error = if let Some(worker) = self.metrics_worker.lock().await.take() {
            worker.stop().await.err()
        } else {
            None
        };
        let resolved_error = if let Some(worker) = self.resolved_worker.lock().await.take() {
            worker.stop().await.err()
        } else {
            None
        };
        let mut guard = self.resources.lock().await;
        let result = if let Some(resources) = guard.as_mut() {
            resources.syncer.stop().await;
            for runner in &mut resources.runners {
                runner.stop().await;
            }
            resources.hot.shutdown().await.map_err(Into::into)
        } else {
            Ok(())
        };
        guard.take();
        if let Some(error) = schedule_error
            .or(worker_error)
            .or(stats_error)
            .or(config_error)
            .or(metrics_error)
            .or(resolved_error)
        {
            return Err(error.into());
        }
        result
    }
}
#[async_trait::async_trait]
impl StoreHeartbeatService for ClusterHost {
    async fn store_heartbeat(&self, stats: StoreStats) -> Result<NodeState, StoreHeartbeatError> {
        let resources = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err(StoreHeartbeatError::Other("node stopped".into()));
        }
        let resources = resources
            .as_ref()
            .ok_or_else(|| StoreHeartbeatError::Other("node stopped".into()))?;
        resources
            .leader
            .validate()
            .await
            .map_err(|e| StoreHeartbeatError::Other(e.to_string()))?;
        let mut state = resources.state.lock().unwrap();
        apply_store_heartbeat(&mut state.cluster, Some(&resources.hot), &stats, now().1)
    }
}
#[async_trait::async_trait]
impl RegionHeartbeatService for ClusterHost {
    async fn bind_heartbeat(
        &self,
        store_id: u64,
        stream: Arc<dyn astersql_pd_schedule_hbstream::routing::HeartbeatStream>,
    ) -> Result<(), RegionHeartbeatError> {
        if self.cancel.is_cancelled() {
            return Err(RegionHeartbeatError("node stopped".into()));
        }
        self.schedule_routing
            .bind(
                store_id,
                Arc::new(schedule_worker::TermStream {
                    inner: stream,
                    cancel: self.cancel.clone(),
                }),
            )
            .await
            .map_err(|e| RegionHeartbeatError(e.to_string()))
    }

    fn region_store_state(&self, id: u64) -> Option<NodeState> {
        self.stores
            .get_store(id)
            .and_then(|s| s.meta().map(|m| m.node_state))
    }
    async fn region_heartbeat(&self, region: RegionInfo) -> Result<(), RegionHeartbeatError> {
        let mut resources = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err(RegionHeartbeatError("node stopped".into()));
        }
        let resources = resources
            .as_mut()
            .ok_or_else(|| RegionHeartbeatError("node stopped".into()))?;
        resources
            .leader
            .validate()
            .await
            .map_err(|e| RegionHeartbeatError(e.to_string()))?;
        let id = region.meta().id;
        let (seconds, nanos) = now();
        let _receipt = resources
            .pipeline()
            .region(region, seconds, nanos)
            .map_err(|e| RegionHeartbeatError(e.to_string()))?;
        resources.schedule_observations.insert(id);
        Ok(())
    }
}
fn now() -> (i64, i64) {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (
        elapsed.as_secs() as i64,
        elapsed.as_nanos().min(i64::MAX as u128) as i64,
    )
}

impl Resources {
    async fn sync_store_configuration(
        &mut self,
        candidates: &mut Vec<Arc<astersql_pd_core::StoreInfo>>,
        cancel: &CancellationToken,
    ) -> Result<(), crate::NodeError> {
        if cancel.is_cancelled() {
            return Ok(());
        }
        self.leader.validate().await?;
        if candidates.is_empty() {
            *candidates = self.state.lock().unwrap().cluster.stores.get_stores();
        }
        let result = sync_store_config(candidates, cancel).await;
        let config = match result {
            Ok(Some(config)) => config,
            other => {
                // Go refreshes the candidate snapshot only when no Store succeeded.
                *candidates = self.state.lock().unwrap().cluster.stores.get_stores();
                return other.map(|_| ()).map_err(Into::into);
            }
        };
        if cancel.is_cancelled() {
            return Ok(());
        }
        self.leader.validate().await?;
        let old = self.state.lock().unwrap().config.store.clone();
        if config == old {
            return Ok(());
        }
        let mut next = self.persisted_config.clone();
        next.store = config;
        if !old.is_raft_kv2() && next.store.is_raft_kv2() {
            next.config.schedule.store_limit_version = "v2".into();
        }
        // Admission is held across this transaction. A failed write never publishes
        // a candidate; cancellation joins admitted writes instead of dropping them.
        next.persist(&self.storage, &self.paths).await?;
        {
            let mut state = self.state.lock().unwrap();
            state.config.store = next.store.clone();
            state.config.schedule = next.config.schedule.clone();
        }
        self.persisted_config = next;
        Ok(())
    }
    fn pipeline(&self) -> StatePipeline<'_, ProviderSnapshot<()>> {
        StatePipeline {
            state: &self.state,
            hot: &self.hot,
            runners: HeartbeatRunners {
                subtree: &self.runners[0],
                misc: &self.runners[1],
                sync: &self.runners[2],
            },
            storage: self.storage.clone(),
            paths: self.paths,
            downstreams: self.downstreams.clone(),
            bucket_enabled: true,
        }
    }
}

#[async_trait::async_trait]
impl astersql_pd_server::ClusterReadService for ClusterHost {
    async fn store(&self, id: u64) -> Result<Option<Arc<astersql_pd_core::StoreInfo>>, String> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let resources = guard.as_ref().ok_or("node stopped")?;
        let state = resources.state.lock().unwrap();
        Ok(state.cluster.stores.get_store(id))
    }
    async fn region(&self, id: u64) -> Result<Option<Arc<RegionInfo>>, String> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let resources = guard.as_ref().ok_or("node stopped")?;
        let state = resources.state.lock().unwrap();
        Ok(state.cluster.regions.get_region(id))
    }
}

#[async_trait::async_trait]
impl astersql_pd_server::bucket_heartbeat_adapter::BucketHeartbeatService for ClusterHost {
    async fn bucket_heartbeat(
        &self,
        report: astersql_pd_statistics_buckets::BucketReport,
    ) -> Result<bool, astersql_pd_cluster::BucketRegionNotFound> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Ok(false);
        }
        let Some(resources) = guard.as_ref() else {
            return Ok(false);
        };
        if resources.leader.validate().await.is_err() {
            return Ok(false);
        }
        resources.pipeline().bucket(report)
    }
}

#[cfg(test)]
#[path = "store_state_worker_test.rs"]
mod store_state_worker_test;

#[cfg(test)]
#[path = "store_stats_worker_test.rs"]
mod store_stats_worker_test;

#[cfg(test)]
#[path = "cluster_metrics_worker_test.rs"]
mod cluster_metrics_worker_test;
#[cfg(test)]
#[path = "store_config_worker_test.rs"]
mod store_config_worker_test;

#[cfg(test)]
#[path = "resolved_ts_worker_test.rs"]
mod resolved_ts_worker_test;

#[path = "schedule_worker.rs"]
mod schedule_worker;

#[path = "schedule_policy.rs"]
mod schedule_policy;

pub use schedule_worker::{SchedulePhase, ScheduleStatus};
