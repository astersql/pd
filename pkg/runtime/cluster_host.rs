// Copyright 2026 AsterSQL.
//! Shared canonical cluster state owned by the serving scope.
use astersql_pd_cluster::{HeartbeatEffectsState, HeartbeatRunners, StatePipeline, StateSnapshot};
use astersql_pd_core::{BasicCluster, NodeState, RegionInfo, StoreStats};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_ratelimit::ConcurrentRunner;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::RuleManager;
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
    state: Arc<State>,
    hot: HotStat,
    runners: [ConcurrentRunner; 3],
    storage: Arc<StorageEndpoint>,
    paths: ClusterPaths,
    downstreams: Downstreams,
}
/// One cache and pipeline shared by all adapters and host readers.
/// The node creates workers on its supplied executor and joins them on shutdown.
/// Read handles retain no storage or workers after shutdown.
pub struct ClusterHost {
    state: Weak<State>,
    restored_config: Option<astersql_pd_server_config::PersistedConfig>,
    resources: AsyncMutex<Option<Resources>>,
    cancel: CancellationToken,
}
impl ClusterHost {
    pub(crate) fn new(
        cluster: BasicCluster,
        config: ProviderSnapshot<()>,
        restored_config: Option<astersql_pd_server_config::PersistedConfig>,
        store: Arc<dyn MetaStore>,
        cluster_id: u64,
        cancel: CancellationToken,
        registry: &prometheus::Registry,
    ) -> Result<Self, crate::NodeError> {
        let storage = Arc::new(StorageEndpoint::new(store));
        let paths = ClusterPaths::new(cluster_id);
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
            rules: RuleManager::new(storage.clone(), paths),
            affinity: None,
        }));
        Ok(Self {
            state: Arc::downgrade(&state),
            restored_config,
            resources: AsyncMutex::new(Some(Resources {
                state,
                hot,
                runners,
                storage,
                paths,
                downstreams: Downstreams::new(HistoryBuffer::new(100)?),
            })),
            cancel,
        })
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
        let mut guard = self.resources.lock().await;
        let result = if let Some(resources) = guard.as_mut() {
            for runner in &mut resources.runners {
                runner.stop().await;
            }
            resources.hot.shutdown().await.map_err(Into::into)
        } else {
            Ok(())
        };
        guard.take();
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
        let mut state = resources.state.lock().unwrap();
        apply_store_heartbeat(&mut state.cluster, Some(&resources.hot), &stats, now().1)
    }
}
#[async_trait::async_trait]
impl RegionHeartbeatService for ClusterHost {
    fn region_store_state(&self, id: u64) -> Option<NodeState> {
        self.state
            .upgrade()?
            .lock()
            .unwrap()
            .cluster
            .stores
            .get_store(id)
            .and_then(|s| s.meta().map(|m| m.node_state))
    }
    async fn region_heartbeat(&self, region: RegionInfo) -> Result<(), RegionHeartbeatError> {
        let resources = self.resources.lock().await;
        if self.cancel.is_cancelled() {
            return Err(RegionHeartbeatError("node stopped".into()));
        }
        let resources = resources
            .as_ref()
            .ok_or_else(|| RegionHeartbeatError("node stopped".into()))?;
        let (seconds, nanos) = now();
        let _receipt = resources
            .pipeline()
            .region(region, seconds, nanos)
            .map_err(|e| RegionHeartbeatError(e.to_string()))?;
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
