// Copyright 2026 AsterSQL.
//! Shared canonical cluster state owned by the serving scope.
use astersql_pd_cluster::{
    HeartbeatEffectsState, HeartbeatRunners, StatePipeline, StateSnapshot, StoreConfigWorker,
    StoreStateWorker, StoreStatsWorker, check_store_states, refresh_store_stats,
    sync_store_config_observed,
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
    recovery: Arc<AsyncMutex<astersql_pd_unsaferecovery::Controller>>,
    recovery_halted: bool,
    recovery_cleanup_error: Option<String>,
    encryption_keys: Option<Arc<astersql_pd_encryption::DictionaryStorage>>,
    encryption_error: Option<String>,
    replication: Option<Arc<astersql_pd_replication::ModeManager>>,
    replication_transport_config: Option<Arc<astersql_pd_server_config::Config>>,
    replicated_states: Arc<AsyncMutex<std::collections::BTreeMap<u64, u64>>>,
    replication_error: Mutex<Option<String>>,

    metrics: Option<Arc<astersql_pd_server::service_metrics::DomainMetricHandles>>,
    metrics_error: Mutex<Option<String>>,
    network_metrics: Mutex<std::collections::BTreeMap<u64, prometheus::Counter>>,
    suspect_cache: Arc<astersql_pd_cache::TTLString<[Vec<u8>; 2]>>,
    suspect_ranges: astersql_pd_schedule_checker::SuspectRanges,
    progress: Arc<astersql_pd_progress::Manager>,
    previous_store_limits:
        std::collections::BTreeMap<u64, astersql_pd_schedule_config::StoreLimitConfig>,
    meta_store: Arc<dyn MetaStore>,
    leader: Arc<astersql_pd_member::Leadership>,
    allocator: Arc<astersql_pd_id::Allocator>,
    schedule_creations: std::collections::BTreeSet<u64>,
    schedule_operators: Option<Arc<astersql_pd_schedule_operator::ControllerAdmission>>,
    schedule_observations:
        std::collections::BTreeMap<u64, Arc<astersql_pd_schedule_operator::Operator>>,
    state: Arc<State>,
    hot: HotStat,
    history: hot_history::Maintenance,
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
    mode_worker: AsyncMutex<Option<astersql_pd_replication::ModeWorker>>,
    operator_clock: schedule_operator_read::ClockMap,
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
    config_wake: Arc<tokio::sync::Notify>,
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
    pub async fn encryption_persistence_error(&self) -> Option<String> {
        self.resources
            .lock()
            .await
            .as_ref()
            .and_then(|r| r.encryption_error.clone())
    }
    pub async fn metrics_collection_error(&self) -> Option<String> {
        self.resources
            .lock()
            .await
            .as_ref()
            .and_then(|r| r.metrics_error.lock().unwrap().clone())
    }
    pub(crate) async fn admin_region(
        &self,
        command: astersql_pd_server::http_admin_cache::Command,
    ) -> Result<(u16, serde_json::Value), String> {
        use astersql_pd_server::http_admin_cache::Command;
        let guard = self.resources.lock().await;
        let r = guard.as_ref().ok_or("server not started")?;
        r.leader.validate().await.map_err(|e| e.to_string())?;
        let message = match command {
            Command::Storage(id) => {
                let region = r.state.lock().unwrap().cluster.regions.get_region(id);
                let Some(region) = region else {
                    return Ok((
                        400,
                        serde_json::json!("failed to get target region from cache"),
                    ));
                };
                let storage =
                    StorageEndpoint::new(Arc::new(crate::member_role_worker::TermStore::new(
                        r.meta_store.clone(),
                        r.leader.clone(),
                        self.cancel.clone(),
                    )));
                if storage
                    .delete_region(&r.paths, region.meta())
                    .await
                    .is_err()
                {
                    return Ok((
                        200,
                        serde_json::json!("failed to delete region from storage."),
                    ));
                }
                r.state
                    .lock()
                    .unwrap()
                    .cluster
                    .regions
                    .remove_region_if_exist(id);
                "The region is removed from server cache and region meta storage."
            }
            Command::Cache(Some(id)) => {
                r.state
                    .lock()
                    .unwrap()
                    .cluster
                    .regions
                    .remove_region_if_exist(id);
                "The region is removed from server cache."
            }
            Command::Cache(None) => {
                r.state.lock().unwrap().cluster.regions.reset_region_cache();
                "All regions are removed from server cache."
            }
        };
        Ok((200, serde_json::json!(message)))
    }

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
        let replication_status = if bootstrapped {
            let manager = r
                .replication
                .as_ref()
                .ok_or("replication manager is not configured")?;
            Some(astersql_pd_server::replication_mode::compact_status(
                &manager.snapshot().await,
            ))
        } else {
            None
        };
        let max_peer_count = r.persisted_config.config.replication.max_replicas as u32;
        let state = r.state.lock().unwrap();
        let regions = &state.cluster.regions;
        let initialized = regions.tree_len() > 1
            || regions.region_by_key(&[], false).is_some_and(|r| {
                r.voters().len() >= max_peer_count as usize && r.pending_peers().is_empty()
            });
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
        member_id: u64,
        cancel: CancellationToken,
        registry: &prometheus::Registry,
        leader: Arc<astersql_pd_member::Leadership>,
        allocator: Arc<astersql_pd_id::Allocator>,
        metrics: Option<Arc<astersql_pd_server::service_metrics::DomainMetricHandles>>,
        encryption: Option<Arc<crate::encryption::EncryptionResources>>,
        recovery_steps: Arc<astersql_pd_unsaferecovery::RecoverySteps>,
    ) -> Result<Self, crate::NodeError> {
        let _start_metrics = metrics.as_ref().map(|m| {
            m.classic
                .raft_cluster_start_duration
                .with_label_values(&["non-bootstrap"])
                .start_timer()
        });
        let operator_clock = schedule_operator_read::ClockMap::new();
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
        let encryption_keys = encryption.as_ref().map(|keys| keys.dictionary.clone());
        if let Some(keys) = &encryption {
            persisted_config.config.security.encryption = keys.config.clone();
        }
        let storage = Arc::new(
            StorageEndpoint::new(store.clone()).with_encryption_keys(encryption_keys.clone()),
        );
        let paths = ClusterPaths::new(cluster_id);
        let replication = if storage.load_meta(&paths).await?.is_some() {
            Some(
                replication_mode::manager(
                    persisted_config.config.replication_mode.clone(),
                    store.clone(),
                    paths,
                    allocator.clone(),
                    cancel.clone(),
                    metrics.as_ref().map(|m| m.replication.clone()),
                )
                .await?,
            )
        } else {
            None
        };
        let has_replication = replication.is_some();

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
        for store in cluster
            .stores
            .get_stores()
            .iter()
            .filter(|s| !s.is_removed())
        {
            hot.stores_stats
                .get_or_create_rolling_store_stats(store.id());
        }
        let mut runners = std::array::from_fn(|_| ConcurrentRunner::new(None));
        for (runner, name) in
            runners
                .iter_mut()
                .zip(["heartbeat-async", "misc-async", "sync-region-async"])
        {
            if let Some(metrics) = &metrics {
                runner.set_metrics(astersql_pd_ratelimit::RunnerMetrics::new(
                    &metrics.classic,
                    name,
                    &[
                        "",
                        "UpdateSubTree",
                        "HandleOverlaps",
                        "ObserveRegionStatsAsync",
                        "CollectRegionStatsAsync",
                        "SaveRegionToKV",
                        "SyncRegionToFollower",
                    ],
                ));
            }
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
        if let Some(metrics) = &metrics {
            syncer.downstreams().attach_metrics(metrics.sync.clone());
        }
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
        let suspect_cache = Arc::new(astersql_pd_cache::TTLString::new(
            tokio::runtime::Handle::current(),
            cancel.clone(),
            std::time::Duration::from_secs(60),
            astersql_pd_cache::TtlDuration::from_nanos(180_000_000_000),
        ));
        if let Some(metrics) = &metrics {
            if let Some(stats) = state.lock().unwrap().region_stats.as_mut() {
                stats.set_metrics(metrics.statistics.clone());
            }
        }
        let resources = Arc::new(AsyncMutex::new(Some(Resources {
            recovery: Arc::new(AsyncMutex::new(
                astersql_pd_unsaferecovery::Controller::new(recovery_steps),
            )),
            recovery_halted: false,
            recovery_cleanup_error: None,
            encryption_keys: encryption_keys.clone(),
            encryption_error: None,
            replication,
            replication_transport_config: None,
            replicated_states: Arc::new(AsyncMutex::new(Default::default())),
            replication_error: Mutex::new(None),
            metrics: metrics.clone(),
            metrics_error: Mutex::new(None),
            network_metrics: Mutex::new(Default::default()),
            suspect_ranges: astersql_pd_schedule_checker::SuspectRanges::new(suspect_cache.clone()),
            suspect_cache,
            progress: progress.clone(),
            previous_store_limits: Default::default(),
            history: hot_history::Maintenance::new(
                store.clone(),
                cluster_id,
                member_id,
                std::time::Duration::from_nanos(
                    state
                        .lock()
                        .unwrap()
                        .config
                        .schedule
                        .hot_regions_write_interval
                        .as_nanos()
                        .max(1) as u64,
                ),
            )
            .with_encryption(encryption_keys),
            meta_store: store,
            leader,
            allocator,
            schedule_creations: Default::default(),
            schedule_operators: None,
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
        let mode_worker = has_replication.then(|| {
            replication_mode::start(
                resources.clone(),
                cancel.clone(),
                metrics.as_ref().map(|m| m.replication.clone()),
            )
        });
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
        let stats_worker = StoreStatsWorker::start(
            &tokio::runtime::Handle::current(),
            &cancel,
            move || {
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
                    let begin = std::time::Instant::now();
                    if let Some(metrics) = &resources.metrics {
                        let (ids, active) = {
                            let state = resources.state.lock().unwrap();
                            (
                                state
                                    .cluster
                                    .regions
                                    .get_regions()
                                    .iter()
                                    .map(|r| r.meta().id)
                                    .collect(),
                                state
                                    .cluster
                                    .stores
                                    .get_stores()
                                    .iter()
                                    .filter(|s| !s.is_removed())
                                    .map(|s| s.id())
                                    .collect::<std::collections::BTreeSet<_>>(),
                            )
                        };
                        let result = tokio::select! {biased;
                            _=cancel.cancelled()=>return,
                            result=resources.hot.hot_cache.collect_metrics(&metrics.statistics,now().1,&active)=>result,
                        };
                        *resources.metrics_error.lock().unwrap() =
                            result.err().map(|e| e.to_string());
                        if let Ok(snapshot) = resources.hot.snapshot(0, ids).await {
                            let read = snapshot.read.into_iter().collect();
                            let write = snapshot.write.into_iter().collect();
                            let state = resources.state.lock().unwrap();
                            let stores = state
                                .cluster
                                .stores
                                .get_stores()
                                .iter()
                                .map(|s| s.as_ref().clone())
                                .collect::<Vec<_>>();
                            let infos = astersql_pd_statistics::summary_store_infos(&stores);
                            for rw in [
                                astersql_pd_statistics_utils::RWType::READ,
                                astersql_pd_statistics_utils::RWType::WRITE,
                            ] {
                                for kind in [
                                    astersql_pd_core_constant::ResourceKind::LEADER,
                                    astersql_pd_core_constant::ResourceKind::REGION,
                                ] {
                                    let details = astersql_pd_statistics::summary_stores_load(
                                        &infos,
                                        &snapshot.stores,
                                        None,
                                        if rw == astersql_pd_statistics_utils::RWType::READ {
                                            &read
                                        } else {
                                            &write
                                        },
                                        true,
                                        rw,
                                        kind,
                                        std::time::Duration::from_nanos(now().1 as u64),
                                    )
                                    .into_values()
                                    .collect::<Vec<_>>();
                                    metrics
                                        .statistics
                                        .collect_hot_load_summary(&details, rw, kind);
                                }
                            }
                            metrics.hotspot.collect_hot_spot_metrics(
                                &state.cluster,
                                &stores,
                                &astersql_pd_schedule::HotObservationInputs {
                                    loads: &snapshot.stores,
                                    read: &read,
                                    write: &write,
                                },
                            );
                        }
                    }
                    let mut state = resources.state.lock().unwrap();
                    refresh_store_stats(&state.cluster);
                    let regions = state.cluster.regions.get_regions();
                    let labels = state.config.replication.location_labels.clone();
                    let labels = labels
                        .as_slice()
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>();
                    for region in regions {
                        let stores = state.cluster.stores.get_region_stores(&region);
                        let stores = stores
                            .iter()
                            .filter(|store| !store.is_tiflash_write())
                            .map(AsRef::as_ref)
                            .collect::<Vec<_>>();
                        state.label_stats.observe(&region, &stores, &labels);
                    }
                    state.label_stats.clear_defunct_regions();
                    if let Some(metrics) = &resources.metrics {
                        let mut census =
                            astersql_pd_statistics::StoreStatisticsMap::new(&state.config);
                        for store in state.cluster.stores.get_stores() {
                            census.observe(&store, now().1 as i128);
                            metrics
                                .statistics
                                .observe_hot_stat(&store, &resources.hot.stores_stats);
                        }
                        metrics
                            .statistics
                            .collect_store_statistics(&census.collect())
                            .expect("canonical metric labels");
                        if let Some(stats) = &state.region_stats {
                            metrics.statistics.collect_region_statistics(stats);
                        }
                        metrics
                            .statistics
                            .collect_label_statistics(&state.label_stats);
                        metrics
                            .classic
                            .update_store_stats_gauge
                            .set(begin.elapsed().as_secs_f64());
                    }
                }
            },
        );
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
                metrics.as_ref().map(|m| m.heartbeat.clone()),
            );
        let progress_cancel = cancel.clone();
        let progress_worker = tokio::runtime::Handle::current().spawn(async move {
            progress.gc(&progress_cancel).await;
        });
        Ok(Self {
            mode_worker: AsyncMutex::new(mode_worker),
            operator_clock,
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
            config_wake: config_worker.wake_handle(),
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
        let mut guard = self.resources.lock().await;
        let resources = guard.as_mut().ok_or("node stopped")?;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let bootstrap_begin = std::time::Instant::now();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos() as u64;
        let endpoint =
            StorageEndpoint::new(backend).with_encryption_keys(resources.encryption_keys.clone());
        let meta = astersql_pd_storage_endpoint::Cluster {
            id: resources.paths.cluster_id(),
            max_peer_count: resources.persisted_config.config.replication.max_replicas as u32,
        };
        let committed = endpoint
            .bootstrap_cluster(&resources.paths, &meta, &store, &region, nanos)
            .await
            .map_err(|e| e.to_string())?;
        if committed {
            // Go first commits bootstrap metadata, then saves its region through
            // the configured storage adapter; a failed secondary save is recorded.
            if let Some(keys) = &resources.encryption_keys {
                match keys.get_current_key() {
                    Ok(Some(_)) => {
                        if let Err(error) = endpoint.save_region(&resources.paths, &region).await {
                            resources.encryption_error = Some(error.to_string());
                        }
                    }
                    Ok(None) => {}
                    Err(error) => resources.encryption_error = Some(error.to_string()),
                }
            }
            let manager = replication_mode::manager(
                resources.persisted_config.config.replication_mode.clone(),
                resources.meta_store.clone(),
                resources.paths,
                resources.allocator.clone(),
                self.cancel.clone(),
                resources.metrics.as_ref().map(|m| m.replication.clone()),
            )
            .await
            .map_err(|e| e.to_string())?;
            resources.replication = Some(manager);
            if let Some(metrics) = &resources.metrics {
                metrics
                    .classic
                    .raft_cluster_start_duration
                    .with_label_values(&["bootstrap"])
                    .observe(bootstrap_begin.elapsed().as_secs_f64());
            }
            resources
                .hot
                .stores_stats
                .get_or_create_rolling_store_stats(store.id);
            {
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
            self.config_wake.notify_one();
            let mut worker = self.mode_worker.lock().await;
            if worker.is_none() && !self.cancel.is_cancelled() {
                *worker = Some(replication_mode::start(
                    self.resources.clone(),
                    self.cancel.clone(),
                    resources.metrics.as_ref().map(|m| m.replication.clone()),
                ));
            }
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
    /// Observe the same reports and initialization predicate used by the worker.
    pub(crate) async fn resolved_ts_observation(
        &self,
    ) -> Result<astersql_pd_server::http_resolved_ts::Observation, String> {
        let guard = self.resources.lock().await;
        let r = guard.as_ref().ok_or("server not started")?;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let state = r.state.lock().unwrap();
        let regions = &state.cluster.regions;
        let initialized = regions.tree_len() > 1
            || regions.region_by_key(&[], false).is_some_and(|region| {
                region.voters().len() >= state.config.replication.max_replicas as usize
                    && region.pending_peers().is_empty()
            });
        Ok(astersql_pd_server::http_resolved_ts::Observation {
            minimum: if initialized {
                self.min_resolved_ts()
            } else {
                u64::MAX
            },
            stores: state
                .cluster
                .stores
                .get_stores()
                .iter()
                .map(|s| {
                    (
                        s.id(),
                        if initialized && s.is_available_for_min_resolved_ts() {
                            s.min_resolved_ts()
                        } else {
                            u64::MAX
                        },
                    )
                })
                .collect(),
        })
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
        resources
            .hot
            .stores_stats
            .get_or_create_rolling_store_stats(registered_id);
        store_limits::ensure_registered_limit(registered_id, resources).await;
        self.update_split_version(resources).await;
        Ok(())
    }
    pub(crate) async fn shutdown(&self) -> Result<(), crate::NodeError> {
        self.cancel.cancel();
        let mode_error = if let Some(worker) = self.mode_worker.lock().await.take() {
            worker.stop().await.err()
        } else {
            None
        };
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
            resources.suspect_cache.shutdown().await;
            resources.syncer.stop().await;
            for runner in &mut resources.runners {
                runner.stop().await;
            }
            resources.hot.shutdown().await.map_err(Into::into)
        } else {
            Ok(())
        };
        guard.take();
        if let Some(error) = mode_error
            .or(schedule_error)
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
    fn recovery_enabled(&self) -> bool {
        true
    }
    async fn recovery_heartbeat(
        &self,
        store: u64,
        report: Option<astersql_pd_unsaferecovery::StoreReport>,
    ) -> Result<(NodeState, Option<u64>), StoreHeartbeatError> {
        let guard = self.resources.lock().await;
        let r = guard
            .as_ref()
            .ok_or_else(|| StoreHeartbeatError::Other("node stopped".into()))?;
        r.leader
            .validate()
            .await
            .map_err(|e| StoreHeartbeatError::Other(e.to_string()))?;
        let store_info = r
            .state
            .lock()
            .unwrap()
            .cluster
            .stores
            .get_store(store)
            .ok_or(StoreHeartbeatError::StoreNotFound(store))?;
        if store_info.is_removed() {
            return Err(StoreHeartbeatError::Removed);
        }
        let mut controller = r.recovery.lock().await;
        controller
            .collect_report(store, report)
            .map_err(|e| StoreHeartbeatError::Other(e.to_string()))?;
        Ok((
            store_info.node_state(),
            controller.request_report(store, now().1 as i128),
        ))
    }

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
        let result =
            apply_store_heartbeat(&mut state.cluster, Some(&resources.hot), &stats, now().1)?;
        if state
            .cluster
            .stores
            .get_store(stats.store_id)
            .is_some_and(|s| !s.is_removed())
        {
            let slow = state.cluster.stores.avg_network_slow_score(stats.store_id) >= 99;
            if slow {
                state
                    .cluster
                    .stores
                    .trigger_network_slow_evict(stats.store_id);
            } else {
                state
                    .cluster
                    .stores
                    .reset_trigger_network_slow_evict(stats.store_id);
            }
            if let Some(metrics) = &resources.metrics {
                let mut children = resources.network_metrics.lock().unwrap();
                if slow {
                    children
                        .entry(stats.store_id)
                        .or_insert_with(|| {
                            metrics
                                .classic
                                .store_trigger_network_slow_evict
                                .with_label_values(&[&stats.store_id.to_string()])
                        })
                        .inc();
                } else {
                    children.remove(&stats.store_id);
                    let _ = metrics
                        .classic
                        .store_trigger_network_slow_evict
                        .remove_label_values(&[&stats.store_id.to_string()]);
                }
                if state
                    .cluster
                    .stores
                    .get_store(stats.store_id)
                    .is_none_or(|s| s.is_removed())
                {
                    children.remove(&stats.store_id);
                    let _ = metrics
                        .classic
                        .store_trigger_network_slow_evict
                        .remove_label_values(&[&stats.store_id.to_string()]);
                }
            }
        }
        Ok(result)
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
        resources
            .state
            .lock()
            .unwrap()
            .cluster
            .regions
            .metrics()
            .set_breakdown_metrics(
                resources
                    .persisted_config
                    .config
                    .schedule
                    .enable_heartbeat_breakdown_metrics,
            );
        let _receipt = resources
            .pipeline()
            .region(region, seconds, nanos)
            .map_err(|e| RegionHeartbeatError(e.to_string()))?;
        // Dispatch only to the operation present when this report was accepted.
        // An old queued heartbeat must not act on a later admission/replacement.
        if let Some(operator) = resources
            .schedule_operators
            .as_ref()
            .and_then(|operators| operators.get_operator(id))
        {
            resources.schedule_observations.insert(id, operator);
        }
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
        let failure = |address: &str| {
            if let Some(metrics) = &self.metrics {
                metrics.classic.store_config_metrics(address, "fail");
            }
        };
        let result = sync_store_config_observed(candidates, cancel, Some(&failure)).await;
        let report = match result {
            Ok(Some(report)) => report,
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
        if report.config == old {
            if let Some(metrics) = &self.metrics {
                metrics
                    .classic
                    .store_config_metrics(&report.address, "succ");
            }
            return Ok(());
        }
        let mut next = self.persisted_config.clone();
        next.store = report.config;
        if !old.is_raft_kv2() && next.store.is_raft_kv2() {
            next.config.schedule.store_limit_version = "v2".into();
        }
        // Admission is held across this transaction. A failed write never publishes
        // a candidate; cancellation joins admitted writes instead of dropping them.
        if let Err(error) = next.persist(&self.storage, &self.paths).await {
            failure(&report.address);
            return Err(error.into());
        }
        if let Some(metrics) = &self.metrics {
            if !old.is_raft_kv2() && next.store.is_raft_kv2() {
                metrics
                    .classic
                    .store_config_metrics(&report.address, "raft-v2");
            }
            metrics
                .classic
                .store_config_metrics(&report.address, "succ");
        }
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
    async fn region_anomalies(
        &self,
        kind: astersql_pd_statistics::RegionStatisticType,
    ) -> Result<Vec<Arc<RegionInfo>>, String> {
        let guard = self.resources.lock().await;
        let r = guard.as_ref().ok_or("node stopped")?;
        let state = r.state.lock().unwrap();
        Ok(state
            .region_stats
            .as_ref()
            .map(|stats| {
                stats
                    .region_ids_by_type(kind)
                    .into_iter()
                    .filter_map(|id| state.cluster.regions.get_region(id))
                    .collect()
            })
            .unwrap_or_default())
    }
    async fn region_range_holes(&self) -> Result<Vec<[String; 2]>, String> {
        let guard = self.resources.lock().await;
        let r = guard.as_ref().ok_or("node stopped")?;
        Ok(r.state.lock().unwrap().cluster.regions.get_range_holes())
    }
    async fn regions_replicated(&self, start: &str, end: &str) -> Result<String, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::select! {biased;_=self.cancel.cancelled()=>Err("server not started".into()),result=async{
            self.schedule_requests.send(schedule_scatter::Command::Replicated(start.into(),end.into(),tx)).await.map_err(|_|"scheduling stopped")?;
            rx.await.map_err(|_|"scheduling stopped")?
        }=>result}
    }

    async fn hotspot(
        &self,
        query: astersql_pd_server::http_hotspot::Query,
    ) -> Result<serde_json::Value, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::select! {biased; _=self.cancel.cancelled()=>Err("server not started".into()), result=async {
            self.schedule_requests.send(schedule_scatter::Command::Hotspot(query, tx)).await.map_err(|_|"scheduling stopped")?;
            rx.await.map_err(|_|"scheduling stopped")?
        }=>result}
    }

    async fn region_statistics(
        &self,
        start: &[u8],
        end: &[u8],
        count: bool,
        hot: bool,
        engine: &str,
    ) -> Result<astersql_pd_statistics::RegionStats, String> {
        use astersql_pd_schedule_filter::{
            EngineFilter, Filter, not_special_engines, special_engines,
        };
        use astersql_pd_statistics::{RegionStats, with_store_map_option};
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let resources = guard.as_ref().ok_or("node stopped")?;
        let (regions, stores, config) = {
            let state = resources.state.lock().unwrap();
            if count {
                return Ok(RegionStats {
                    count: state.cluster.regions.get_region_count_by_range(start, end) as i64,
                    ..Default::default()
                });
            }
            (
                state.cluster.regions.scan_regions(start, end, 0),
                state.cluster.stores.get_stores(),
                state.config.clone(),
            )
        };
        let snapshot = if hot {
            Some(
                resources
                    .hot
                    .snapshot(0, vec![])
                    .await
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let engine_filter = if hot {
            match engine {
                "tikv" => Some(EngineFilter::new("user", not_special_engines())),
                "tiflash" => Some(EngineFilter::new("user", special_engines())),
                _ => None,
            }
        } else {
            None
        };
        let stores: std::collections::BTreeMap<_, _> = stores
            .into_iter()
            .filter(|s| {
                engine_filter
                    .as_ref()
                    .is_none_or(|filter| filter.source_metrics(&config, s).is_ok())
            })
            .map(|s| (s.id(), s.label_value("engine").to_owned()))
            .collect();
        let filter = with_store_map_option(&stores);
        let mut stats = RegionStats::default();
        for (i, r) in regions.iter().enumerate() {
            if i % 256 == 0 {
                tokio::task::yield_now().await;
            }
            if self.cancel.is_cancelled() {
                return Err("node stopped".into());
            }
            let filters: [astersql_pd_statistics::GetRegionStatsOption<'_>; 1] = [&filter];
            stats.observe(
                r,
                snapshot
                    .as_ref()
                    .map(|s| s as &dyn astersql_pd_statistics::RegionStatInformer),
                if hot { &filters } else { &[] },
            );
        }
        if hot {
            for (&id, label) in &stores {
                macro_rules! zero {($($f:ident),*)=>{$(stats.$f.entry(id).or_default();)*};}
                zero!(
                    store_leader_count,
                    store_leader_keys,
                    store_leader_size,
                    store_peer_count,
                    store_peer_keys,
                    store_peer_size,
                    store_write_bytes,
                    store_write_keys,
                    store_write_query,
                    store_leader_read_keys,
                    store_leader_read_bytes,
                    store_leader_read_query,
                    store_peer_read_keys,
                    store_peer_read_bytes,
                    store_peer_read_query
                );
                stats.store_engine.insert(
                    id,
                    if label.is_empty() {
                        "tikv".into()
                    } else {
                        label.clone()
                    },
                );
            }
        }
        Ok(stats)
    }

    async fn region_list(
        &self,
        store: Option<(u64, Option<astersql_pd_core::RegionRole>)>,
    ) -> Result<Vec<Arc<astersql_pd_core::RegionInfo>>, String> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let state = guard.as_ref().ok_or("node stopped")?.state.lock().unwrap();
        Ok(match store {
            Some((id, role)) => state.cluster.regions.store_regions(id, role),
            None => state.cluster.regions.get_regions(),
        })
    }
    async fn region_count(&self) -> Result<usize, String> {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let state = guard.as_ref().ok_or("node stopped")?.state.lock().unwrap();
        Ok(state.cluster.regions.total_region_count())
    }

    async fn adjacent_regions(
        &self,
        region: &astersql_pd_core::RegionInfo,
    ) -> Result<
        (
            Option<Arc<astersql_pd_core::RegionInfo>>,
            Option<Arc<astersql_pd_core::RegionInfo>>,
        ),
        String,
    > {
        let guard = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
        }
        let state = guard.as_ref().ok_or("node stopped")?.state.lock().unwrap();
        Ok(state.cluster.regions.get_adjacent_regions(region))
    }

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

#[path = "schedule_check.rs"]
mod schedule_check;

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

#[path = "hot_history.rs"]
mod hot_history;

#[path = "schedule_operator_read.rs"]
mod schedule_operator_read;

#[path = "schedule_operator_write.rs"]
mod schedule_operator_write;

#[path = "schedule_scheduler.rs"]
mod schedule_scheduler;

#[path = "placement_http.rs"]
mod placement_http;

#[path = "region_label_http.rs"]
mod region_label_http;

#[path = "affinity_read_http.rs"]
mod affinity_read_http;

#[path = "affinity_write_http.rs"]
mod affinity_write_http;

#[path = "replication_mode.rs"]
mod replication_mode;
