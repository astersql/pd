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
    progress: Arc<astersql_pd_progress::Manager>,
    previous_store_limits:
        std::collections::BTreeMap<u64, astersql_pd_schedule_config::StoreLimitConfig>,
    meta_store: Arc<dyn MetaStore>,
    leader: Arc<astersql_pd_member::Leadership>,
    allocator: Arc<astersql_pd_id::Allocator>,
    schedule_creations: std::collections::BTreeSet<u64>,
    schedule_observations: std::collections::BTreeSet<u64>,
    state: Arc<State>,
    hot: HotStat,
    runners: [ConcurrentRunner; 3],
    storage: Arc<StorageEndpoint>,
    paths: ClusterPaths,
    downstreams: Downstreams,
    syncer: Arc<RegionSyncer>,
    persisted_config: astersql_pd_server_config::PersistedConfig,
    resolved_interval: Arc<std::sync::atomic::AtomicU64>,
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
    progress_worker: AsyncMutex<Option<tokio::task::JoinHandle<()>>>,
    stats_worker: AsyncMutex<Option<StoreStatsWorker>>,
    config_worker: AsyncMutex<Option<StoreConfigWorker>>,
    metrics_worker: AsyncMutex<Option<astersql_pd_cluster::ClusterMetricsWorker>>,
    schedule_requests: tokio::sync::mpsc::Sender<schedule_scatter::Command>,
    schedule_status: tokio::sync::watch::Receiver<ScheduleStatus>,
    schedule_worker: AsyncMutex<Option<schedule_worker::ScheduleWorker>>,
    schedule_routing: astersql_pd_schedule_hbstream::routing::RoutingHandle,
    schedule_error: Arc<Mutex<Option<String>>>,
    schedule_initialized: Arc<std::sync::atomic::AtomicBool>,
    resolved_worker: AsyncMutex<Option<astersql_pd_cluster::ResolvedTsWorker>>,
    external_ts: std::sync::atomic::AtomicU64,
    min_resolved_ts: Arc<std::sync::atomic::AtomicU64>,
    resolved_error: Arc<Mutex<Option<String>>>,
    store_config_error: Arc<Mutex<Option<String>>>,
    store_check_errors: Arc<Mutex<Vec<(u64, String)>>>,
    cancel: CancellationToken,
}
impl ClusterHost {
    pub(crate) async fn store_progress(
        &self,
        id: Option<u64>,
        action: String,
    ) -> Result<serde_json::Value, String> {
        let guard = self.resources.lock().await;
        let r = guard.as_ref().ok_or("server not started")?;
        let p = if let Some(id) = id {
            r.progress.get_progress_by_store_id(id)
        } else {
            r.progress.get_average_progress_by_action(&action)
        };
        let p = p.ok_or_else(|| {
            format!(
                "[PD:progress:ErrProgressNotFound]no progress found for {}",
                id.map(|id| format!("the given store ID: {id}"))
                    .unwrap_or_else(|| format!("the action: {action}"))
            )
        })?;
        for n in [p.progress_percent, p.current_speed, p.left_second] {
            if !n.is_finite() {
                return Err(format!(
                    "json: unsupported value: {}",
                    if n.is_nan() {
                        "NaN"
                    } else if n.is_sign_positive() {
                        "+Inf"
                    } else {
                        "-Inf"
                    }
                ));
            }
        }
        let mut v = serde_json::json!({"action":p.action,"progress":p.progress_percent,"current_speed":p.current_speed,"left_seconds":p.left_second});
        if let Some(id) = id.filter(|id| *id != 0) {
            v["store_id"] = serde_json::json!(id);
        }
        Ok(v)
    }

    pub(crate) async fn configuration(&self) -> Result<astersql_pd_server_config::Config, String> {
        let guard = self.resources.lock().await;
        let resources = guard.as_ref().ok_or("server not started")?;
        if self.cancel.is_cancelled() {
            return Err("server not started".into());
        }
        let mut config = resources.persisted_config.config.clone();
        let state = resources.state.lock().unwrap();
        config.schedule = state.config.schedule.clone();
        config.replication = state.config.replication.clone();
        Ok(config)
    }

    pub(crate) async fn http_observation(
        &self,
    ) -> Result<astersql_pd_server::http_cluster::ClusterObservation, String> {
        let guard = self.resources.lock().await;
        let r = guard.as_ref().ok_or("server not started")?;
        if self.cancel.is_cancelled() {
            return Err("server not started".into());
        }
        let bootstrapped = r
            .storage
            .load_meta(&r.paths)
            .await
            .map_err(|e| e.to_string())?
            .is_some();
        let max_peer_count = r.persisted_config.config.replication.max_replicas as u32;
        let state = r.state.lock().unwrap();
        let regions = &state.cluster.regions;
        let initialized = regions.tree_len() > 1
            || regions.region_by_key(&[], false).is_some_and(|r| {
                r.voters().len() >= max_peer_count as usize && r.pending_peers().is_empty()
            });
        // Bootstrap currently admits only the majority manager. Never invent a DR state.
        let replication_status = if bootstrapped {
            if r.persisted_config.config.replication_mode.replication_mode != "majority" {
                return Err("replication manager is not configured".into());
            }
            // Gogo CompactTextString omits the default MAJORITY enum.
            Some(String::new())
        } else {
            None
        };
        Ok(astersql_pd_server::http_cluster::ClusterObservation {
            max_peer_count,
            initialized,
            replication_status,
        })
    }

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
        let storage = Arc::new(StorageEndpoint::new(store.clone()));
        let paths = ClusterPaths::new(cluster_id);
        let external_ts =
            std::sync::atomic::AtomicU64::new(storage.load_external_ts(&paths).await.unwrap_or(0));
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
        {
            // Scatter uses the canonical default voter rule even when placement rules
            // are disabled, so the term must initialize the shared rule manager.
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
        let progress = Arc::new(astersql_pd_progress::Manager::new(
            Arc::new(|| std::time::Duration::ZERO),
            std::time::Duration::from_secs(10),
            registry,
        )?);
        let resources = Arc::new(AsyncMutex::new(Some(Resources {
            progress: progress.clone(),
            previous_store_limits: Default::default(),
            meta_store: store,
            leader,
            allocator,
            schedule_creations: Default::default(),
            schedule_observations: Default::default(),
            state: state.clone(),
            hot,
            runners,
            storage,
            paths,
            persisted_config,
            resolved_interval: resolved_interval.clone(),
            downstreams: syncer.downstreams(),
            syncer,
        })));
        let (schedule_requests, schedule_rx) = tokio::sync::mpsc::channel(64);
        let cleanup_requests = schedule_requests.clone();
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
                let cleanup_requests = cleanup_requests.clone();
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
                    let candidates = resources
                        .state
                        .lock()
                        .unwrap()
                        .cluster
                        .stores
                        .get_stores()
                        .into_iter()
                        .filter(|s| s.is_removing() || s.is_removed())
                        .map(|s| s.id())
                        .collect::<Vec<_>>();
                    let samples = astersql_pd_cluster::store_progress_samples(
                        &resources.state.lock().unwrap(),
                        prepared,
                    );
                    let result = check_store_states(
                        &resources.state,
                        &resources.storage,
                        resources.paths,
                        now().1 as i128,
                        prepared,
                    )
                    .await;
                    *errors.lock().unwrap() = result;
                    for (store, threshold) in samples {
                        resources.progress.update_progress(
                            Some(&store),
                            store.region_size() as f64,
                            threshold,
                        );
                    }
                    {
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
                    drop(guard);
                    if !candidates.is_empty() {
                        tokio::select! {biased;_=cancel.cancelled()=>{},_=cleanup_requests.send(schedule_scatter::Command::StoreCleanup(candidates))=>{}}
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
                schedule_rx,
                scheduling_cluster,
                scheduling_configuration,
                cancel.clone(),
                schedule_error.clone(),
                schedule_initialized.clone(),
                cluster_id,
            );
        let progress_cancel = cancel.clone();
        let progress_worker = tokio::runtime::Handle::current().spawn(async move {
            progress.gc(&progress_cancel).await;
        });
        Ok(Self {
            progress_worker: AsyncMutex::new(Some(progress_worker)),
            stores,
            schedule_requests,
            schedule_status,
            schedule_worker: AsyncMutex::new(Some(schedule_worker)),
            schedule_routing,
            schedule_error,
            schedule_initialized,
            resolved_worker: AsyncMutex::new(Some(resolved_worker)),
            external_ts,
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
    /// Serialize readiness with bootstrap publication and term teardown.
    pub(crate) async fn regions_initialized(&self) -> bool {
        let resources = self.resources.lock().await;
        resources.is_some() && !self.cancel.is_cancelled()
    }
    pub(crate) async fn bootstrap(
        &self,
        backend: Arc<dyn MetaStore>,
        store: astersql_pd_core::StoreMeta,
        region: astersql_pd_core::RegionMeta,
    ) -> Result<bool, String> {
        let guard = self.resources.lock().await;
        let resources = guard.as_ref().ok_or("node stopped")?;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        if resources
            .persisted_config
            .config
            .replication_mode
            .replication_mode
            != "majority"
        {
            return Err(
                "bootstrap replication mode requires a configured replication manager".into(),
            );
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos() as u64;
        let endpoint = StorageEndpoint::new(backend);
        let meta = astersql_pd_storage_endpoint::Cluster {
            id: resources.paths.cluster_id(),
            max_peer_count: resources.persisted_config.config.replication.max_replicas as u32,
        };
        let committed = endpoint
            .bootstrap_cluster(&resources.paths, &meta, &store, &region, nanos)
            .await
            .map_err(|e| e.to_string())?;
        if committed {
            let mut state = resources.state.lock().unwrap();
            state
                .cluster
                .stores
                .put_store(Arc::new(astersql_pd_core::StoreInfo::new(store)));
            state
                .cluster
                .regions
                .check_and_put_region(Arc::new(RegionInfo::new(region, None, vec![], vec![])));
            state.cluster.update_all_store_status();
        }
        Ok(committed)
    }
    /// Return the current term's canonical external timestamp.
    pub async fn external_timestamp(&self) -> Result<u64, String> {
        let resources = self.resources.lock().await;
        if self.cancel.is_cancelled() || resources.is_none() {
            return Err("node stopped".into());
        }
        Ok(self.external_ts.load(std::sync::atomic::Ordering::SeqCst))
    }
    /// Publish before persistence, preserving Go's failure side effect.
    pub async fn set_external_timestamp(&self, timestamp: u64) -> Result<(), String> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let resources = guard.as_ref().ok_or("node stopped")?;
        resources
            .leader
            .validate()
            .await
            .map_err(|e| e.to_string())?;
        if timestamp <= self.external_ts.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(
                "the external timestamp should be larger than current external timestamp".into(),
            );
        }
        self.external_ts
            .store(timestamp, std::sync::atomic::Ordering::SeqCst);
        let storage = StorageEndpoint::new(Arc::new(crate::member_role_worker::TermStore::new(
            resources.meta_store.clone(),
            resources.leader.clone(),
            self.cancel.clone(),
        )));
        storage
            .save_external_ts(&resources.paths, timestamp)
            .await
            .map_err(|e| e.to_string())
    }
    /// Replace a store report under the same lock as lifecycle transitions.
    pub async fn report_min_resolved_ts(&self, store_id: u64, ts: u64) -> Result<(), String> {
        let resources = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let resources = resources.as_ref().ok_or("node stopped")?;
        resources
            .leader
            .validate()
            .await
            .map_err(|e| e.to_string())?;
        let state = resources.state.lock().unwrap();
        let store = state
            .cluster
            .stores
            .get_store(store_id)
            .ok_or_else(|| format!("[PD:core:ErrStoreNotFound]store {store_id} not found"))?;
        if store.is_removed() {
            return Err(format!(
                "[PD:core:ErrStoreRemoved]store {store_id} has been removed"
            ));
        }
        state
            .cluster
            .stores
            .patch_store(store_id, |store| store.set_min_resolved_ts(ts));
        Ok(())
    }
    /// Return metadata from the canonical heartbeat view.
    pub async fn registered_stores(&self) -> Result<Vec<astersql_pd_core::StoreMeta>, String> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let resources = guard.as_ref().ok_or("node stopped")?;
        let state = resources.state.lock().unwrap();
        Ok(state
            .cluster
            .stores
            .get_stores()
            .iter()
            .filter_map(|s| s.meta().cloned())
            .collect())
    }
    /// Validate, persist and publish a registration into the canonical view.
    pub async fn register_store(
        &self,
        incoming: astersql_pd_core::StoreMeta,
    ) -> Result<(), astersql_pd_server::grpc_store::StoreError> {
        use astersql_pd_server::grpc_store::StoreError;
        // Serialize validation, durable write and publication against other registrations.
        let mut guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err(StoreError::Other("node stopped".into()));
        }
        let resources = guard
            .as_mut()
            .ok_or_else(|| StoreError::Other("node stopped".into()))?;
        let next = {
            let state = resources.state.lock().unwrap();
            let previous = state.cluster.stores.get_store(incoming.id);
            if previous.as_ref().is_some_and(|s| s.is_removed()) {
                return Err(StoreError::Removed);
            }
            astersql_pd_core::validate_store_engine_key(Some(&incoming))
                .map_err(|e| StoreError::InvalidEngine(e.to_string()))?;
            let config = &resources.persisted_config.config;
            if !config.replication.enable_placement_rules
                && astersql_pd_core::is_store_contain_label(Some(&incoming), "engine", "tiflash")
            {
                return Err(StoreError::Other("placement rules is disabled".into()));
            }
            check_store_admission(&incoming, &config.cluster_version, &state.cluster)?;
            let next = match previous {
                None => astersql_pd_core::StoreInfo::new(incoming),
                Some(old) => {
                    let mut next = old.as_ref().clone();
                    next.edit_meta(|meta| {
                        meta.address = incoming.address;
                        meta.peer_address = incoming.peer_address;
                        meta.status_address = incoming.status_address;
                        meta.version = incoming.version;
                        meta.git_hash = incoming.git_hash;
                        meta.start_timestamp = incoming.start_timestamp;
                        meta.deploy_path = incoming.deploy_path;
                        meta.labels =
                            astersql_pd_core::merge_labels(&meta.labels, &incoming.labels);
                    });
                    next
                }
            };
            astersql_pd_cluster::check_store_location_labels(
                &next,
                config.replication.location_labels.as_slice(),
                config.replication.strictly_match_label,
            )
            .map_err(StoreError::Other)?;
            next
        };
        let registered_id = next.id();
        resources
            .storage
            .save_store_meta(&resources.paths, next.meta().unwrap())
            .await
            .map_err(|e| StoreError::Other(e.to_string()))?;
        {
            let state = resources.state.lock().unwrap();
            // Keep concurrently accepted heartbeat statistics and lifecycle options.
            if !state.cluster.stores.patch_store(next.id(), |store| {
                let incoming = next.meta().unwrap();
                store.edit_meta(|meta| {
                    meta.address.clone_from(&incoming.address);
                    meta.peer_address.clone_from(&incoming.peer_address);
                    meta.status_address.clone_from(&incoming.status_address);
                    meta.version.clone_from(&incoming.version);
                    meta.git_hash.clone_from(&incoming.git_hash);
                    meta.start_timestamp = incoming.start_timestamp;
                    meta.deploy_path.clone_from(&incoming.deploy_path);
                    meta.labels.clone_from(&incoming.labels);
                });
            }) {
                state.cluster.stores.put_store(Arc::new(next));
            }
        }
        store_limits::ensure_registered_limit(registered_id, resources).await;
        self.update_split_version(resources).await;
        Ok(())
    }
    pub(crate) async fn shutdown(&self) -> Result<(), crate::NodeError> {
        self.cancel.cancel();
        let progress_error = if let Some(worker) = self.progress_worker.lock().await.take() {
            worker.await.err()
        } else {
            None
        };
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
            .or(progress_error)
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
    async fn query_regions(
        &self,
        keys: &[Vec<u8>],
        prev_keys: &[Vec<u8>],
        ids: &[u64],
        need_buckets: bool,
    ) -> Result<Option<astersql_pd_core::RegionQueryResult>, String> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let state = guard.as_ref().ok_or("node stopped")?.state.lock().unwrap();
        let keys: Vec<_> = keys.iter().map(Vec::as_slice).collect();
        let prev_keys: Vec<_> = prev_keys.iter().map(Vec::as_slice).collect();
        Ok(Some(state.cluster.regions.query_regions(
            &keys,
            &prev_keys,
            ids,
            need_buckets,
        )))
    }

    async fn batch_scan_regions(
        &self,
        ranges: &[astersql_pd_core::KeyRange],
        options: astersql_pd_core::BatchScanRegionsOptions,
    ) -> Result<
        Result<Vec<Arc<astersql_pd_core::RegionInfo>>, astersql_pd_core::RegionNotAdjacent>,
        String,
    > {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let state = guard.as_ref().ok_or("node stopped")?.state.lock().unwrap();
        Ok(state.cluster.regions.batch_scan_regions(ranges, options))
    }
    async fn store(&self, id: u64) -> Result<Option<Arc<astersql_pd_core::StoreInfo>>, String> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let resources = guard.as_ref().ok_or("node stopped")?;
        let state = resources.state.lock().unwrap();
        Ok(state.cluster.stores.get_store(id))
    }
    async fn scan_regions(
        &self,
        start: &[u8],
        end: &[u8],
        limit: isize,
    ) -> Result<Vec<Arc<astersql_pd_core::RegionInfo>>, String> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let state = guard.as_ref().ok_or("node stopped")?.state.lock().unwrap();
        Ok(state.cluster.regions.scan_regions(start, end, limit))
    }
    async fn region_by_key(
        &self,
        key: &[u8],
        previous: bool,
    ) -> Result<Option<Arc<RegionInfo>>, String> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let resources = guard.as_ref().ok_or("node stopped")?;
        let state = resources.state.lock().unwrap();
        Ok(state.cluster.regions.region_by_key(key, previous))
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

#[path = "cluster_split.rs"]
mod cluster_split;

#[path = "schedule_scatter.rs"]
mod schedule_scatter;

#[path = "schedule_split.rs"]
mod schedule_split;

#[path = "replication_config.rs"]
mod replication_config;

#[cfg(test)]
#[path = "replication_config_test.rs"]
mod replication_config_test;

#[path = "server_config.rs"]
mod server_config;

#[cfg(test)]
#[path = "server_config_test.rs"]
mod server_config_test;

#[path = "store_lifecycle.rs"]
mod store_lifecycle;

#[path = "store_labels.rs"]
mod store_labels;

fn check_store_admission(
    incoming: &astersql_pd_core::StoreMeta,
    version: &astersql_pd_server_config::ClusterVersion,
    cluster: &BasicCluster,
) -> Result<(), astersql_pd_server::grpc_store::StoreError> {
    use astersql_pd_server::grpc_store::StoreError;
    if incoming.id == 0 {
        return Err(StoreError::InvalidStore(Box::new(incoming.clone())));
    }
    let text = if incoming.version.is_empty() {
        "1.0.0"
    } else {
        incoming
            .version
            .strip_prefix('v')
            .unwrap_or(&incoming.version)
    };
    let parsed: astersql_pd_server_config::ClusterVersion =
        serde_json::from_value(serde_json::Value::String(text.into()))
            .map_err(|e| StoreError::Other(format!("invalid put store, error: {e}")))?;
    let current = version;
    if (parsed.major, parsed.minor) < (current.major, current.minor) {
        return Err(StoreError::Other(format!(
            "version should compatible with version  {current}, got {parsed}"
        )));
    }
    for store in cluster.stores.get_stores() {
        if store.id() == incoming.id || store.is_removed() || store.is_physically_destroyed() {
            continue;
        }
        let old = store.meta().unwrap();
        if incoming.address == old.address
            || (!old.peer_address.is_empty() && incoming.address == old.peer_address)
        {
            return Err(StoreError::AddressConflict {
                peer: false,
                incoming: Box::new(incoming.clone()),
                existing: Box::new(old.clone()),
            });
        }
        if !incoming.peer_address.is_empty()
            && (incoming.peer_address == old.address || incoming.peer_address == old.peer_address)
        {
            return Err(StoreError::AddressConflict {
                peer: true,
                incoming: Box::new(incoming.clone()),
                existing: Box::new(old.clone()),
            });
        }
    }
    Ok(())
}

#[path = "store_limits.rs"]
mod store_limits;
