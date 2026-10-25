// Copyright 2026 AsterSQL.
//! Specialized consumers of the host's shared cluster and canonical statistics.
use crate::{BasicCluster, PrepareRegionCount, SharedCluster, SharedIdAllocator};
use astersql_pd_core::{BasicCluster as CoreCluster, RegionInfo, RegionsInfo, StoresInfo};
use astersql_pd_schedule_affinity::manager::Manager as AffinityManager;
use astersql_pd_schedule_config::{
    CheckerConfigProvider, SchedulerConfigProvider, SharedConfigProvider, StoreConfigProvider,
};
use astersql_pd_schedule_keyrange::Manager as KeyRangeManager;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::{
    HotPeerSnapshot, LabelStatistics, RegionStatInformer, StoreKindLoads, StoreStatInformer,
};
use astersql_pd_statistics_buckets::{
    BucketStats, BucketSubmitError, BucketWaitError, BucketWorkerClient,
};
use astersql_pd_statistics_utils::RWType;
use std::{collections::HashMap, future::Future};
use tokio_util::sync::CancellationToken;

/// A bucket query either fails admission or fails waiting; no partial map is returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BucketQueryError {
    Submit(BucketSubmitError),
    Wait(BucketWaitError),
}
impl std::fmt::Display for BucketQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Submit(e) => e.fmt(f),
            Self::Wait(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for BucketQueryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Submit(e) => Some(e),
            Self::Wait(e) => Some(e),
        }
    }
}

/// Store configuration shared by scheduler and checker consumers.
pub trait StoreConfigCluster: SharedCluster {
    type StoreConfig: StoreConfigProvider + ?Sized;
    fn get_store_config(&self) -> &Self::StoreConfig;
}
/// Scheduler observations. Bucket reads use the canonical host-runtime worker.
pub trait SchedulerCluster: StoreConfigCluster + StoreStatInformer {
    type SchedulerConfig: SchedulerConfigProvider + ?Sized;
    fn get_scheduler_config(&self) -> &Self::SchedulerConfig;
    fn bucket_statistics(&self) -> &BucketWorkerClient;

    /// Admit a FIFO read after preceding reports. Cancellation only stops this wait;
    /// it never cancels the worker or discards accepted reports.
    fn buckets_stats(
        &self,
        degree: i32,
        region_ids: Vec<u64>,
        cancellation: &CancellationToken,
    ) -> impl Future<Output = Result<BucketStats, BucketQueryError>> + Send {
        let query = self.bucket_statistics().collect(degree, region_ids);
        let cancellation = cancellation.clone();
        async move {
            query
                .map_err(BucketQueryError::Submit)?
                .wait(&cancellation)
                .await
                .map_err(BucketQueryError::Wait)
        }
    }
}
/// Checker observations and serialized updates of the host's label statistics.
pub trait CheckerCluster: StoreConfigCluster {
    type CheckerConfig: CheckerConfigProvider + ?Sized;
    fn get_checker_config(&self) -> &Self::CheckerConfig;
    fn update_regions_label_level_stats(&mut self, regions: &[&RegionInfo]);
}

/// One borrowed host generation implementing both specialized contracts.
///
/// Consumers accept `SchedulerCluster` or `CheckerCluster` independently. Config
/// getters all return the shared provider, so publication cannot mix generations.
/// The host mutates/publishes configuration between borrows, supplies canonical
/// store/bucket owners and exclusively lends label statistics for checker updates.
/// No server, protocol, metadata backend, cache copy or private runtime is needed.
pub struct ClusterViews<'a, S: SharedCluster> {
    pub shared: S,
    pub store_statistics: &'a (dyn StoreStatInformer + Sync),
    pub bucket_statistics: &'a BucketWorkerClient,
    pub label_statistics: &'a mut LabelStatistics,
}
impl<S: SharedCluster> BasicCluster for ClusterViews<'_, S> {
    fn stores(&self) -> &StoresInfo {
        self.shared.stores()
    }
    fn regions(&self) -> &RegionsInfo {
        self.shared.regions()
    }
}
impl<S: SharedCluster> RegionStatInformer for ClusterViews<'_, S> {
    fn get_hot_peer_stat(&self, rw: RWType, region: u64, store: u64) -> Option<HotPeerSnapshot> {
        self.shared.get_hot_peer_stat(rw, region, store)
    }
}
impl<S: SharedCluster> SharedCluster for ClusterViews<'_, S> {
    type Config = S::Config;
    type Preparation = S::Preparation;
    type Allocation = S::Allocation;
    fn get_basic_cluster(&self) -> &CoreCluster {
        self.shared.get_basic_cluster()
    }
    fn get_shared_config(&self) -> &Self::Config {
        self.shared.get_shared_config()
    }
    fn get_region_labeler(&self) -> &RegionLabeler {
        self.shared.get_region_labeler()
    }
    fn get_rule_manager(&self) -> &RuleManager {
        self.shared.get_rule_manager()
    }
    fn get_key_range_manager(&self) -> &KeyRangeManager {
        self.shared.get_key_range_manager()
    }
    fn get_affinity_manager(&self) -> &AffinityManager {
        self.shared.get_affinity_manager()
    }
    fn id_allocation(&self) -> &Self::Allocation {
        self.shared.id_allocation()
    }
    fn prepare_region_count_provider(&self) -> &Self::Preparation {
        self.shared.prepare_region_count_provider()
    }
    fn is_scheduling_halted(&self) -> bool {
        self.shared.is_scheduling_halted()
    }
    fn alloc_id(
        &self,
        count: u32,
    ) -> impl Future<Output = Result<(u64, u32), <Self::Allocation as SharedIdAllocator>::Error>> + Send
    {
        self.shared.alloc_id(count)
    }
    fn get_prepare_region_count(
        &self,
    ) -> impl Future<Output = Result<usize, <Self::Preparation as PrepareRegionCount>::Error>> + Send
    {
        self.shared.get_prepare_region_count()
    }
}
impl<S: SharedCluster> StoreStatInformer for ClusterViews<'_, S> {
    fn get_stores_loads(&self) -> HashMap<u64, StoreKindLoads> {
        self.store_statistics.get_stores_loads()
    }
}
impl<S: SharedCluster> StoreConfigCluster for ClusterViews<'_, S>
where
    S::Config: StoreConfigProvider,
{
    type StoreConfig = S::Config;
    fn get_store_config(&self) -> &Self::StoreConfig {
        self.get_shared_config()
    }
}
impl<S: SharedCluster> SchedulerCluster for ClusterViews<'_, S>
where
    S::Config: SchedulerConfigProvider + StoreConfigProvider,
{
    type SchedulerConfig = S::Config;
    fn get_scheduler_config(&self) -> &Self::SchedulerConfig {
        self.get_shared_config()
    }
    fn bucket_statistics(&self) -> &BucketWorkerClient {
        self.bucket_statistics
    }
}
impl<S: SharedCluster> CheckerCluster for ClusterViews<'_, S>
where
    S::Config: CheckerConfigProvider,
{
    type CheckerConfig = S::Config;
    fn get_checker_config(&self) -> &Self::CheckerConfig {
        self.get_shared_config()
    }
    fn update_regions_label_level_stats(&mut self, regions: &[&RegionInfo]) {
        let labels = self.shared.get_shared_config().get_location_labels();
        let labels: Vec<_> = labels.iter().map(String::as_str).collect();
        for region in regions {
            let stores: Vec<_> = region
                .meta()
                .peers
                .iter()
                .filter_map(|peer| self.shared.get_store(peer.store_id))
                .filter(|store| !store.is_tiflash_write())
                .collect();
            let stores: Vec<_> = stores.iter().map(AsRef::as_ref).collect();
            self.label_statistics.observe(region, &stores, &labels);
        }
        self.label_statistics.clear_defunct_regions();
    }
}
